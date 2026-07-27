use std::collections::{BTreeMap, BTreeSet};

use codegraph_core::{
    stable_fact_identity_key, stable_micro_edge_id, stable_micro_node_id, Entity, EntityKind,
    Exactness, MicroDerivationKind, MicroEdgeCandidate, MicroEdgeIdentityInput, MicroEdgeKind,
    MicroExactness, MicroFactProvenance, MicroNodeIdentityInput, MicroNodeKind, MicroSourceRole,
    SourceSpan, MVP4_3_PARSER_FACTS_V1_CLAIMABILITY,
    MVP4_3_PARSER_FACTS_V1_MICRO_FACT_EXTRACTION_VERSION,
};
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::{
    direct_call_callee_name, generic_decl_kind, node_contains,
    node_has_error_or_missing_descendant, node_text, source_span_for_node, structural_path_to_node,
    Mvp4MicroNodeCandidate, ParsedFile, ParserFactBundle, SourceLanguage,
};

const PARSER_FACTS_V1_EXTRACTION_VERSION: &str = "mvp4-parser-facts-v1";
const PARSER_FACTS_V1_ROW_SCHEMA_VERSION: u32 = 1;
const PARSER_FACTS_V1_PAYLOAD_VERSION: u32 = 1;
const PARSER_FACTS_V1_CLAIMABILITY: &str = "non_claimable_parser_facts_v1_inactive";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParserFactsV1Gap {
    pub gap_kind: String,
    pub reason: String,
    pub source_span: Option<SourceSpan>,
    pub function_identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParserFactsV1Report {
    pub engine_id: String,
    pub status: String,
    pub repo_relative_path: String,
    pub language: String,
    pub frontend: String,
    pub source_role: MicroSourceRole,
    pub production_activation_enabled: bool,
    pub nodes: Vec<Mvp4MicroNodeCandidate>,
    pub edges: Vec<MicroEdgeCandidate>,
    pub gaps: Vec<ParserFactsV1Gap>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ParserFactsV1;

impl ParserFactsV1 {
    pub fn extract(
        parsed: &ParsedFile,
        source: &str,
        parser_fact_bundle: &ParserFactBundle,
    ) -> ParserFactsV1Report {
        Engine::new(parsed, source, parser_fact_bundle).extract()
    }

    pub fn extract_active(
        parsed: &ParsedFile,
        source: &str,
        parser_fact_bundle: &ParserFactBundle,
    ) -> ParserFactsV1Report {
        let mut report = Self::extract(parsed, source, parser_fact_bundle);
        if report.source_role != MicroSourceRole::Production || report.status != "complete_inactive"
        {
            return report;
        }

        report.engine_id = MVP4_3_PARSER_FACTS_V1_MICRO_FACT_EXTRACTION_VERSION.to_string();
        report.status = "complete_active".to_string();
        report.production_activation_enabled = true;
        for node in &mut report.nodes {
            node.claimability = MVP4_3_PARSER_FACTS_V1_CLAIMABILITY.to_string();
            node.extraction_version =
                MVP4_3_PARSER_FACTS_V1_MICRO_FACT_EXTRACTION_VERSION.to_string();
            node.provenance.extractor_or_adapter_version =
                MVP4_3_PARSER_FACTS_V1_MICRO_FACT_EXTRACTION_VERSION.to_string();
            node.provenance
                .limitations
                .retain(|limitation| limitation != "parser_facts_v1_is_not_production_activated");
            node.provenance
                .limitations
                .push("same_file_intraprocedural_static_semantics_only".to_string());
        }
        for edge in &mut report.edges {
            edge.claimability = MVP4_3_PARSER_FACTS_V1_CLAIMABILITY.to_string();
            edge.extraction_version =
                MVP4_3_PARSER_FACTS_V1_MICRO_FACT_EXTRACTION_VERSION.to_string();
            edge.provenance.extractor_or_adapter_version =
                MVP4_3_PARSER_FACTS_V1_MICRO_FACT_EXTRACTION_VERSION.to_string();
            edge.provenance
                .limitations
                .retain(|limitation| limitation != "parser_facts_v1_is_not_production_activated");
            edge.provenance
                .limitations
                .push("same_file_intraprocedural_static_semantics_only".to_string());
        }
        report
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerKind {
    Sanitizer,
    Assertion,
}

#[derive(Debug, Clone)]
struct FunctionScope<'tree> {
    entity: Entity,
    ast_node: Node<'tree>,
    frame_id: String,
    scope_path: Vec<String>,
}

#[derive(Debug, Clone)]
struct BindingRef {
    entity_id: String,
    candidate_id: String,
    node_kind: MicroNodeKind,
    declaration_span: SourceSpan,
}

#[derive(Debug, Clone)]
struct ExpressionResultSource {
    candidate_id: String,
    exact_fact_ids: Vec<String>,
}

#[derive(Debug)]
struct FunctionAnalysis<'tree> {
    scope: FunctionScope<'tree>,
    owned_nodes: Vec<Node<'tree>>,
    implicit_return_expressions: Vec<Node<'tree>>,
    bindings_by_name: BTreeMap<String, Vec<BindingRef>>,
    non_value_flow_bindings: BTreeSet<String>,
    candidate_by_ast_and_kind: BTreeMap<(usize, MicroNodeKind), String>,
}

impl<'tree> FunctionAnalysis<'tree> {
    fn is_implicit_return_expression(&self, node: Node<'tree>) -> bool {
        self.implicit_return_expressions
            .iter()
            .any(|expression| expression.id() == node.id())
    }

    fn is_return_site(&self, node: Node<'tree>) -> bool {
        is_return_node(node) || self.is_implicit_return_expression(node)
    }

    fn return_value_expression(&self, node: Node<'tree>) -> Option<Node<'tree>> {
        if is_return_node(node) {
            return return_expression(node);
        }
        self.is_implicit_return_expression(node).then_some(node)
    }
}

struct Engine<'a> {
    parsed: &'a ParsedFile,
    source: &'a str,
    bundle: &'a ParserFactBundle,
    source_role: MicroSourceRole,
    report: ParserFactsV1Report,
    node_ids: BTreeSet<String>,
    edge_ids: BTreeSet<String>,
}

impl<'a> Engine<'a> {
    fn new(parsed: &'a ParsedFile, source: &'a str, bundle: &'a ParserFactBundle) -> Self {
        let source_role = super::mvp4_typescript_source_role(&parsed.repo_relative_path).role;
        let frontend =
            codegraph_core::mvp4_language_frontend_contract(&bundle.file_identity.language)
                .map(|contract| contract.frontend)
                .unwrap_or(bundle.file_identity.frontend.as_str())
                .to_string();
        Self {
            parsed,
            source,
            bundle,
            source_role,
            report: ParserFactsV1Report {
                engine_id: PARSER_FACTS_V1_EXTRACTION_VERSION.to_string(),
                status: "complete_inactive".to_string(),
                repo_relative_path: parsed.repo_relative_path.clone(),
                language: bundle.file_identity.language.clone(),
                frontend,
                source_role,
                production_activation_enabled: false,
                nodes: Vec::new(),
                edges: Vec::new(),
                gaps: Vec::new(),
            },
            node_ids: BTreeSet::new(),
            edge_ids: BTreeSet::new(),
        }
    }

    fn extract(mut self) -> ParserFactsV1Report {
        if self.parsed.has_syntax_errors() {
            self.report.status = "parse_recovery_excluded".to_string();
            self.push_gap(
                "parse_recovery",
                "syntax-error or missing-node recovery cannot produce ParserFactsV1 semantics",
                Some(self.parsed.root_node.source_span.clone()),
                None,
            );
            return self.report;
        }

        let (parent_by_child, ambiguous_parents) = bundle_parent_chain(self.bundle);
        for entity_id in ambiguous_parents {
            let span = self
                .bundle
                .entity_facts
                .iter()
                .find(|fact| fact.entity.id == entity_id)
                .and_then(|fact| fact.source_span.clone());
            self.push_gap(
                "ambiguous_bundle_parent",
                format!("bundle entity `{entity_id}` has multiple CONTAINS parents"),
                span,
                None,
            );
        }

        let mut functions = self.function_scopes(&parent_by_child);
        if functions.is_empty() {
            self.report.status = "no_exact_function_scope".to_string();
            self.push_gap(
                "missing_function_scope",
                "no exact bundle function could be matched to a source-spanned AST declaration",
                Some(self.parsed.root_node.source_span.clone()),
                None,
            );
            return self.report;
        }

        let function_ids = functions
            .iter()
            .map(|scope| scope.entity.id.clone())
            .collect::<BTreeSet<_>>();
        let mut analyses = functions
            .drain(..)
            .map(|scope| {
                let body = function_body_node(scope.ast_node).unwrap_or(scope.ast_node);
                let mut owned_nodes = Vec::new();
                collect_owned_function_nodes(
                    body,
                    scope.ast_node.id(),
                    self.parsed.language,
                    &mut owned_nodes,
                );
                let implicit_return_expressions = language_implicit_return_expressions(
                    self.parsed.language,
                    scope.ast_node,
                    body,
                    self.source,
                );
                FunctionAnalysis {
                    non_value_flow_bindings: language_non_value_flow_binding_names(
                        self.parsed.language,
                        scope.ast_node,
                        &owned_nodes,
                        self.source,
                    ),
                    scope,
                    owned_nodes,
                    implicit_return_expressions,
                    bindings_by_name: BTreeMap::new(),
                    candidate_by_ast_and_kind: BTreeMap::new(),
                }
            })
            .collect::<Vec<_>>();

        self.add_bundle_bindings(&mut analyses, &function_ids, &parent_by_child);
        self.add_synthesized_parameters(&mut analyses);
        self.add_synthesized_declaration_bindings(&mut analyses);
        self.record_ambiguous_bindings(&analyses);
        self.record_language_semantic_gaps(&analyses);

        let comments = comment_nodes(self.parsed.tree().root_node());
        let marker_by_function_id = analyses
            .iter()
            .filter_map(|analysis| {
                marker_for_function(analysis.scope.ast_node, &comments, self.source)
                    .map(|marker| (analysis.scope.entity.id.clone(), marker))
            })
            .collect::<BTreeMap<_, _>>();
        let function_indices_by_name =
            function_indices_by_name(self.parsed.language, &analyses, self.source);

        for index in 0..analyses.len() {
            self.add_ast_nodes(
                index,
                &mut analyses,
                &marker_by_function_id,
                &function_indices_by_name,
            );
        }
        self.add_exact_relations(&analyses, &function_indices_by_name);
        self.add_derived_relations(&analyses, &function_indices_by_name);
        self.add_marker_relations(&analyses, &function_indices_by_name, &marker_by_function_id);

        self.report.nodes.sort_by(|left, right| {
            left.repo_relative_path
                .cmp(&right.repo_relative_path)
                .then_with(|| {
                    left.source_span
                        .start_line
                        .cmp(&right.source_span.start_line)
                })
                .then_with(|| {
                    left.source_span
                        .start_column
                        .cmp(&right.source_span.start_column)
                })
                .then_with(|| left.node_kind.cmp(&right.node_kind))
                .then_with(|| left.micro_node_id.cmp(&right.micro_node_id))
        });
        self.report.edges.sort_by(|left, right| {
            left.relation_source_span
                .start_line
                .cmp(&right.relation_source_span.start_line)
                .then_with(|| left.micro_edge_kind.cmp(&right.micro_edge_kind))
                .then_with(|| left.micro_edge_id.cmp(&right.micro_edge_id))
        });
        self.report
    }

    fn function_scopes(
        &mut self,
        parent_by_child: &BTreeMap<String, String>,
    ) -> Vec<FunctionScope<'a>> {
        let mut ast_functions = Vec::new();
        collect_function_nodes(
            self.parsed.tree().root_node(),
            self.parsed.language,
            &mut ast_functions,
        );
        let entities_by_id = self
            .bundle
            .entity_facts
            .iter()
            .map(|fact| (fact.entity.id.clone(), fact.entity.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut scopes = Vec::new();

        for ast_node in ast_functions {
            if node_has_error_or_missing_descendant(ast_node) {
                self.push_gap(
                    "function_parse_recovery",
                    "function declaration contains ERROR or MISSING descendants",
                    Some(source_span_for_node(
                        &self.parsed.repo_relative_path,
                        ast_node,
                    )),
                    None,
                );
                continue;
            }
            if is_c_family_language(self.parsed.language) {
                if ast_node.kind() != "function_definition"
                    || function_body_node(ast_node)
                        .is_none_or(|body| body.kind() != "compound_statement")
                {
                    self.push_gap(
                        "non_executable_function_declaration",
                        "C/C++ prototypes and function-pointer declarators are not executable function frames",
                        Some(source_span_for_node(
                            &self.parsed.repo_relative_path,
                            ast_node,
                        )),
                        None,
                    );
                    continue;
                }
                if let Some(boundary) =
                    c_family_claimability_boundary(self.parsed.language, ast_node)
                {
                    self.push_gap(
                        "unsupported_c_family_claimability_region",
                        format!(
                            "C/C++ function is inside unsupported `{boundary}` syntax and cannot emit claimable flow"
                        ),
                        Some(source_span_for_node(
                            &self.parsed.repo_relative_path,
                            ast_node,
                        )),
                        None,
                    );
                    continue;
                }
            }
            if let Some((gap_kind, reason)) =
                ruby_php_function_scope_gap(self.parsed.language, ast_node)
            {
                self.push_gap(
                    gap_kind,
                    reason,
                    Some(source_span_for_node(
                        &self.parsed.repo_relative_path,
                        ast_node,
                    )),
                    None,
                );
                continue;
            }
            let Some(entity_kind) = generic_decl_kind(self.parsed.language, ast_node) else {
                continue;
            };
            if !is_function_entity_kind(entity_kind) {
                continue;
            }
            let span = source_span_for_node(&self.parsed.repo_relative_path, ast_node);
            let matches = self
                .bundle
                .entity_facts
                .iter()
                .filter(|fact| {
                    fact.entity.kind == entity_kind
                        && fact.source_span.as_ref() == Some(&span)
                        && parser_fact_is_exact(fact.exactness)
                        && fact.unknown_boundary_reason.is_none()
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                self.push_gap(
                    "ambiguous_function_entity",
                    format!(
                        "AST function `{}` matched {} exact bundle entities",
                        compact_node_label(ast_node, self.source),
                        matches.len()
                    ),
                    Some(span),
                    None,
                );
                continue;
            }
            let entity = matches[0].entity.clone();
            let scope_path = bundle_scope_path(&entity.id, parent_by_child, &entities_by_id);
            let structural_path = structural_path_to_node(self.parsed.tree().root_node(), ast_node);
            let frame = self.make_candidate(
                MicroNodeKind::FunctionFrame,
                &entity.id,
                &entity.name,
                span,
                Some(entity.id.clone()),
                None,
                scope_path.clone(),
                structural_path,
                vec![entity.id.clone()],
            );
            let frame_id = frame.micro_node_id.clone();
            self.push_node(frame);
            scopes.push(FunctionScope {
                entity,
                ast_node,
                frame_id,
                scope_path,
            });
        }
        scopes.sort_by(|left, right| {
            left.ast_node
                .start_byte()
                .cmp(&right.ast_node.start_byte())
                .then_with(|| left.entity.id.cmp(&right.entity.id))
        });
        scopes
    }

    fn add_bundle_bindings(
        &mut self,
        analyses: &mut [FunctionAnalysis<'a>],
        function_ids: &BTreeSet<String>,
        parent_by_child: &BTreeMap<String, String>,
    ) {
        let function_index_by_id = analyses
            .iter()
            .enumerate()
            .map(|(index, analysis)| (analysis.scope.entity.id.clone(), index))
            .collect::<BTreeMap<_, _>>();
        let binding_facts = self
            .bundle
            .entity_facts
            .iter()
            .filter(|fact| {
                matches!(
                    fact.entity.kind,
                    EntityKind::Parameter | EntityKind::LocalVariable
                ) && fact.source_span.is_some()
                    && parser_fact_is_exact(fact.exactness)
                    && fact.unknown_boundary_reason.is_none()
            })
            .cloned()
            .collect::<Vec<_>>();

        for fact in binding_facts {
            let Some(owner_id) =
                nearest_function_ancestor(&fact.entity.id, parent_by_child, function_ids)
            else {
                self.push_gap(
                    "missing_bundle_binding_owner",
                    format!(
                        "bundle binding `{}` has no exact function ancestor",
                        fact.entity.id
                    ),
                    fact.source_span.clone(),
                    None,
                );
                continue;
            };
            let Some(&analysis_index) = function_index_by_id.get(&owner_id) else {
                continue;
            };
            let binding_name = normalize_binding_name(&fact.entity.name);
            if analyses[analysis_index]
                .non_value_flow_bindings
                .contains(&binding_name)
            {
                continue;
            }
            let span = fact.source_span.clone().expect("filtered source span");
            let binding_ast_node = find_smallest_exact_span_node(
                self.parsed.tree().root_node(),
                &self.parsed.repo_relative_path,
                &span,
            );
            if binding_ast_node.is_some_and(|node| {
                language_binding_context_is_unsupported(
                    self.parsed.language,
                    node,
                    analyses[analysis_index].scope.ast_node,
                )
            }) {
                continue;
            }
            let structural_path = binding_ast_node
                .map(|node| structural_path_to_node(self.parsed.tree().root_node(), node))
                .unwrap_or_else(|| vec!["parser_fact_bundle".to_string(), fact.entity.id.clone()]);
            let kind = if fact.entity.kind == EntityKind::Parameter {
                MicroNodeKind::Parameter
            } else {
                MicroNodeKind::LocalBinding
            };
            let candidate = self.make_candidate(
                kind,
                &owner_id,
                &fact.entity.name,
                span.clone(),
                Some(owner_id.clone()),
                Some(fact.entity.id.clone()),
                analyses[analysis_index].scope.scope_path.clone(),
                structural_path,
                vec![fact.entity.id.clone()],
            );
            let candidate_id = candidate.micro_node_id.clone();
            self.push_node(candidate);
            analyses[analysis_index]
                .bindings_by_name
                .entry(binding_name)
                .or_default()
                .push(BindingRef {
                    entity_id: fact.entity.id,
                    candidate_id,
                    node_kind: kind,
                    declaration_span: span,
                });
        }
    }

    fn add_synthesized_declaration_bindings(&mut self, analyses: &mut [FunctionAnalysis<'a>]) {
        for analysis in analyses {
            let declaration_nodes = analysis
                .owned_nodes
                .iter()
                .copied()
                .filter(|node| {
                    is_synthesizable_declaration_assignment(*node)
                        || is_implicit_local_declaration_node(self.parsed.language, *node)
                })
                .collect::<Vec<_>>();
            for node in declaration_nodes {
                let Some(parts) = assignment_parts(node) else {
                    continue;
                };
                if language_binding_context_is_unsupported(
                    self.parsed.language,
                    parts.left,
                    analysis.scope.ast_node,
                ) {
                    continue;
                }
                if is_property_access_node(self.parsed.language, parts.left) {
                    self.push_gap(
                        "unsupported_binding_declaration",
                        "member, attribute, or indexed assignment cannot declare a local binding",
                        Some(source_span_for_node(
                            &self.parsed.repo_relative_path,
                            parts.left,
                        )),
                        Some(analysis.scope.entity.id.clone()),
                    );
                    continue;
                }
                let Some(name) = simple_binding_name(parts.left, self.source) else {
                    self.push_gap(
                        "unsupported_binding_declaration",
                        "declaration binding is not one exact simple identifier",
                        Some(source_span_for_node(
                            &self.parsed.repo_relative_path,
                            parts.left,
                        )),
                        Some(analysis.scope.entity.id.clone()),
                    );
                    continue;
                };
                if analysis.non_value_flow_bindings.contains(&name) {
                    continue;
                }
                if analysis.bindings_by_name.contains_key(&name) {
                    continue;
                }
                let structural_path =
                    structural_path_to_node(self.parsed.tree().root_node(), parts.left);
                let structural_key = structural_path.join("/");
                let binding_id = stable_fact_identity_key(
                    "parser_facts_v1_binding",
                    [
                        self.parsed.repo_relative_path.as_str(),
                        analysis.scope.entity.id.as_str(),
                        name.as_str(),
                        structural_key.as_str(),
                    ],
                );
                let declaration_span =
                    source_span_for_node(&self.parsed.repo_relative_path, parts.left);
                let candidate = self.make_candidate(
                    MicroNodeKind::LocalBinding,
                    &analysis.scope.entity.id,
                    &name,
                    declaration_span.clone(),
                    Some(analysis.scope.entity.id.clone()),
                    Some(binding_id.clone()),
                    analysis.scope.scope_path.clone(),
                    structural_path,
                    vec![binding_id.clone()],
                );
                let candidate_id = candidate.micro_node_id.clone();
                self.push_node(candidate);
                analysis
                    .bindings_by_name
                    .entry(name)
                    .or_default()
                    .push(BindingRef {
                        entity_id: binding_id,
                        candidate_id,
                        node_kind: MicroNodeKind::LocalBinding,
                        declaration_span,
                    });
            }
        }
    }

    fn add_synthesized_parameters(&mut self, analyses: &mut [FunctionAnalysis<'a>]) {
        for analysis in analyses {
            let Some(parameters) = find_parameter_container(analysis.scope.ast_node) else {
                continue;
            };
            let mut cursor = parameters.walk();
            let parameter_nodes = parameters.named_children(&mut cursor).collect::<Vec<_>>();
            for parameter in parameter_nodes {
                if language_parameter_is_unsupported(self.parsed.language, parameter) {
                    continue;
                }
                if matches!(
                    parameter.kind(),
                    "variadic_parameter" | "variadic_declarator"
                ) || contains_named_kind(parameter, "function_declarator")
                {
                    self.push_gap(
                        "unsupported_parameter_declaration",
                        "variadic or nested function-declarator parameters are not exact",
                        Some(source_span_for_node(
                            &self.parsed.repo_relative_path,
                            parameter,
                        )),
                        Some(analysis.scope.entity.id.clone()),
                    );
                    continue;
                }
                let mut identifiers = Vec::new();
                collect_identifier_nodes(parameter, &mut identifiers);
                identifiers.sort_by_key(|identifier| identifier.start_byte());
                identifiers.dedup_by_key(|identifier| identifier.id());
                if identifiers.len() != 1 {
                    if !identifiers.is_empty() {
                        self.push_gap(
                            "ambiguous_parameter_declaration",
                            "parameter declaration does not contain exactly one declared identifier",
                            Some(source_span_for_node(
                                &self.parsed.repo_relative_path,
                                parameter,
                            )),
                            Some(analysis.scope.entity.id.clone()),
                        );
                    }
                    continue;
                }
                let identifier = identifiers[0];
                let Some(raw_name) = node_text(identifier, self.source) else {
                    continue;
                };
                let name = normalize_binding_name(&raw_name);
                if analysis.non_value_flow_bindings.contains(&name) {
                    continue;
                }
                if analysis.bindings_by_name.contains_key(&name) {
                    continue;
                }
                let structural_path =
                    structural_path_to_node(self.parsed.tree().root_node(), identifier);
                let structural_key = structural_path.join("/");
                let binding_id = stable_fact_identity_key(
                    "parser_facts_v1_parameter",
                    [
                        self.parsed.repo_relative_path.as_str(),
                        analysis.scope.entity.id.as_str(),
                        name.as_str(),
                        structural_key.as_str(),
                    ],
                );
                let declaration_span =
                    source_span_for_node(&self.parsed.repo_relative_path, identifier);
                let candidate = self.make_candidate(
                    MicroNodeKind::Parameter,
                    &analysis.scope.entity.id,
                    &name,
                    declaration_span.clone(),
                    Some(analysis.scope.entity.id.clone()),
                    Some(binding_id.clone()),
                    analysis.scope.scope_path.clone(),
                    structural_path,
                    vec![binding_id.clone()],
                );
                let candidate_id = candidate.micro_node_id.clone();
                self.push_node(candidate);
                analysis
                    .bindings_by_name
                    .entry(name)
                    .or_default()
                    .push(BindingRef {
                        entity_id: binding_id,
                        candidate_id,
                        node_kind: MicroNodeKind::Parameter,
                        declaration_span,
                    });
            }
        }
    }

    fn record_ambiguous_bindings(&mut self, analyses: &[FunctionAnalysis<'a>]) {
        for analysis in analyses {
            for (name, bindings) in &analysis.bindings_by_name {
                let unique_ids = bindings
                    .iter()
                    .map(|binding| binding.entity_id.as_str())
                    .collect::<BTreeSet<_>>();
                if unique_ids.len() > 1 {
                    self.push_gap(
                        "ambiguous_local_binding",
                        format!(
                            "binding name `{name}` has {} exact declarations in one function",
                            unique_ids.len()
                        ),
                        Some(source_span_for_node(
                            &self.parsed.repo_relative_path,
                            analysis.scope.ast_node,
                        )),
                        Some(analysis.scope.entity.id.clone()),
                    );
                }
            }
        }
    }

    fn record_c_family_semantic_gaps(&mut self, analyses: &[FunctionAnalysis<'a>]) {
        if !is_c_family_language(self.parsed.language) {
            return;
        }
        for analysis in analyses {
            if let Some(parameters) = find_parameter_container(analysis.scope.ast_node) {
                let mut cursor = parameters.walk();
                for parameter in parameters.named_children(&mut cursor) {
                    if c_family_declarator_is_pointer_or_reference(parameter) {
                        self.push_gap(
                            "unsupported_pointer_or_reference_alias",
                            "C/C++ pointer/reference parameters are outside ordinary local value-flow proof",
                            Some(source_span_for_node(
                                &self.parsed.repo_relative_path,
                                parameter,
                            )),
                            Some(analysis.scope.entity.id.clone()),
                        );
                    }
                }
            }
            for node in analysis.owned_nodes.iter().copied() {
                if let Some(parts) = assignment_parts(node) {
                    if c_family_indirect_lvalue(self.parsed.language, parts.left) {
                        self.push_gap(
                            "unsupported_indirect_lvalue",
                            "C/C++ field, subscript, or dereference lvalues cannot resolve to or mutate their base local binding",
                            Some(source_span_for_node(
                                &self.parsed.repo_relative_path,
                                parts.left,
                            )),
                            Some(analysis.scope.entity.id.clone()),
                        );
                    } else if c_family_declarator_is_pointer_or_reference(parts.left) {
                        self.push_gap(
                            "unsupported_pointer_or_reference_alias",
                            "C/C++ pointer/reference declarators are outside ordinary local value-flow proof",
                            Some(source_span_for_node(
                                &self.parsed.repo_relative_path,
                                parts.left,
                            )),
                            Some(analysis.scope.entity.id.clone()),
                        );
                    }
                    if let Some(right) = parts.right {
                        for boundary in
                            c_family_pointer_expression_boundaries(self.parsed.language, right)
                        {
                            self.push_gap(
                                "unsupported_pointer_value_flow",
                                "C/C++ dereference or address-of expressions cannot become ordinary local value-flow proof",
                                Some(source_span_for_node(
                                    &self.parsed.repo_relative_path,
                                    boundary,
                                )),
                                Some(analysis.scope.entity.id.clone()),
                            );
                        }
                    }
                } else if analysis.is_return_site(node) {
                    if let Some(expression) = analysis.return_value_expression(node) {
                        for boundary in
                            c_family_pointer_expression_boundaries(self.parsed.language, expression)
                        {
                            self.push_gap(
                                "unsupported_pointer_value_flow",
                                "C/C++ dereference or address-of expressions cannot become ordinary local value-flow proof",
                                Some(source_span_for_node(
                                    &self.parsed.repo_relative_path,
                                    boundary,
                                )),
                                Some(analysis.scope.entity.id.clone()),
                            );
                        }
                    }
                }
            }
        }
    }

    fn record_language_semantic_gaps(&mut self, analyses: &[FunctionAnalysis<'a>]) {
        self.record_c_family_semantic_gaps(analyses);
        if !matches!(
            self.parsed.language,
            SourceLanguage::Ruby | SourceLanguage::Php
        ) {
            return;
        }

        for analysis in analyses {
            if self.parsed.language == SourceLanguage::Ruby {
                let body =
                    function_body_node(analysis.scope.ast_node).unwrap_or(analysis.scope.ast_node);
                if let Some(tail) = ruby_implicit_return_tail(body) {
                    if !is_return_node(tail)
                        && !ruby_implicit_return_tail_is_safe(tail, self.source)
                    {
                        self.push_gap(
                            "unsupported_ruby_implicit_return_shape",
                            "Ruby implicit return is claimable only for one local identifier, static scalar literal, or plain receiverless blockless call tail",
                            Some(source_span_for_node(
                                &self.parsed.repo_relative_path,
                                tail,
                            )),
                            Some(analysis.scope.entity.id.clone()),
                        );
                    }
                }
            }
            if let Some(parameters) = find_parameter_container(analysis.scope.ast_node) {
                let mut parameter_nodes = Vec::new();
                collect_language_semantic_gap_nodes(
                    self.parsed.language,
                    parameters,
                    self.source,
                    &mut parameter_nodes,
                );
                for (node, gap_kind, reason) in parameter_nodes {
                    self.push_gap(
                        gap_kind,
                        reason,
                        Some(source_span_for_node(&self.parsed.repo_relative_path, node)),
                        Some(analysis.scope.entity.id.clone()),
                    );
                }
            }
            if self.parsed.language == SourceLanguage::Php {
                let mut cursor = analysis.scope.ast_node.walk();
                for child in analysis.scope.ast_node.named_children(&mut cursor) {
                    if child.kind() == "reference_modifier" {
                        self.push_gap(
                            "unsupported_php_reference_semantics",
                            "PHP by-reference returns cannot produce exact local value-flow proof",
                            Some(source_span_for_node(&self.parsed.repo_relative_path, child)),
                            Some(analysis.scope.entity.id.clone()),
                        );
                    }
                }
            }
            for node in analysis.owned_nodes.iter().copied() {
                let Some((gap_kind, reason)) =
                    language_semantic_gap(self.parsed.language, node, self.source)
                else {
                    continue;
                };
                self.push_gap(
                    gap_kind,
                    reason,
                    Some(source_span_for_node(&self.parsed.repo_relative_path, node)),
                    Some(analysis.scope.entity.id.clone()),
                );
            }
        }

        if self.parsed.language == SourceLanguage::Ruby {
            let mut hazards = Vec::new();
            collect_file_level_ruby_resolution_hazards(
                self.parsed.tree().root_node(),
                self.source,
                &mut hazards,
            );
            for analysis in analyses {
                for hazard in hazards.iter().copied() {
                    self.push_gap(
                        "unsupported_ruby_file_resolution_domain",
                        "file-level Ruby open-class or metaprogramming syntax makes optimistic local-call resolution non-claimable",
                        Some(source_span_for_node(
                            &self.parsed.repo_relative_path,
                            hazard,
                        )),
                        Some(analysis.scope.entity.id.clone()),
                    );
                }
            }
        }
        if self.parsed.language == SourceLanguage::Php {
            let mut hazards = Vec::new();
            collect_php_resolution_hazards(
                self.parsed.tree().root_node(),
                self.source,
                &mut hazards,
            );
            for analysis in analyses {
                for hazard in hazards.iter().copied() {
                    self.push_gap(
                        "unsupported_php_file_resolution_domain",
                        "PHP include, require, eval, or imported-function syntax makes optimistic local-call resolution non-claimable",
                        Some(source_span_for_node(
                            &self.parsed.repo_relative_path,
                            hazard,
                        )),
                        Some(analysis.scope.entity.id.clone()),
                    );
                }
            }
        }
    }

    fn add_ast_nodes(
        &mut self,
        analysis_index: usize,
        analyses: &mut [FunctionAnalysis<'a>],
        marker_by_function_id: &BTreeMap<String, MarkerKind>,
        function_indices_by_name: &BTreeMap<String, Vec<usize>>,
    ) {
        let nodes = analyses[analysis_index].owned_nodes.clone();
        for node in &nodes {
            if node_has_error_or_missing_descendant(*node) {
                continue;
            }
            if is_property_access_node(self.parsed.language, *node) {
                self.push_ast_candidate(
                    analysis_index,
                    analyses,
                    MicroNodeKind::PropertyAccess,
                    *node,
                    compact_node_label(*node, self.source),
                );
            }
            if is_native_assertion_node(self.parsed.language, *node) {
                self.push_ast_candidate(
                    analysis_index,
                    analyses,
                    MicroNodeKind::TestAssertion,
                    *node,
                    "assert".to_string(),
                );
            }
            if is_call_node(self.parsed.language, *node)
                && (self.parsed.language != SourceLanguage::Php
                    || php_plain_named_call_name(*node, self.source).is_some())
            {
                self.push_ast_candidate(
                    analysis_index,
                    analyses,
                    MicroNodeKind::CallSite,
                    *node,
                    direct_call_callee_name(self.parsed.language, *node, self.source)
                        .unwrap_or_else(|| compact_node_label(*node, self.source)),
                );
                if let Some((target_index, marker)) = resolve_direct_marked_target(
                    self.parsed.language,
                    *node,
                    self.source,
                    &analyses[analysis_index],
                    analyses,
                    function_indices_by_name,
                    marker_by_function_id,
                ) {
                    let kind = match marker {
                        MarkerKind::Sanitizer => MicroNodeKind::SanitizerCall,
                        MarkerKind::Assertion => MicroNodeKind::TestAssertion,
                    };
                    let target_name = analyses[target_index].scope.entity.name.clone();
                    self.push_ast_candidate(analysis_index, analyses, kind, *node, target_name);
                }
            }
            if analyses[analysis_index].is_return_site(*node) {
                self.push_ast_candidate(
                    analysis_index,
                    analyses,
                    MicroNodeKind::ReturnSite,
                    *node,
                    "return".to_string(),
                );
            }
            if let Some(parts) = assignment_parts(*node) {
                self.push_ast_candidate(
                    analysis_index,
                    analyses,
                    MicroNodeKind::AssignmentSite,
                    *node,
                    compact_node_label(parts.left, self.source),
                );
                let binding = assignment_target_binding(
                    self.parsed.language,
                    &analyses[analysis_index],
                    parts,
                    self.source,
                );
                let is_existing_local_write = binding.is_some_and(|binding| {
                    binding.node_kind == MicroNodeKind::LocalBinding
                        && !assignment_is_binding_declaration(
                            self.parsed.language,
                            *node,
                            parts,
                            binding,
                            &self.parsed.repo_relative_path,
                        )
                });
                if is_existing_local_write {
                    self.push_ast_candidate(
                        analysis_index,
                        analyses,
                        MicroNodeKind::MutationSite,
                        *node,
                        compact_node_label(parts.left, self.source),
                    );
                }
            }
            if is_condition_node(*node) {
                let condition = condition_expression(*node).unwrap_or(*node);
                self.push_ast_candidate(
                    analysis_index,
                    analyses,
                    MicroNodeKind::ConditionSite,
                    condition,
                    compact_node_label(condition, self.source),
                );
                for (label, arm) in condition_branch_arms(*node, condition) {
                    self.push_ast_candidate(
                        analysis_index,
                        analyses,
                        MicroNodeKind::BranchArm,
                        arm,
                        label,
                    );
                }
            }
        }

        for node in nodes {
            if !is_identifier_node(node)
                || !identifier_is_value_use(
                    self.parsed.language,
                    node,
                    analyses[analysis_index].scope.ast_node,
                )
            {
                continue;
            }
            let Some(raw_name) = node_text(node, self.source) else {
                continue;
            };
            let name = normalize_binding_name(&raw_name);
            if analyses[analysis_index]
                .non_value_flow_bindings
                .contains(&name)
            {
                let (gap_kind, reason) = if self.parsed.language == SourceLanguage::Php {
                    (
                        "unsupported_php_nonlocal_or_reference_value",
                        format!(
                            "PHP global, static, reference, or by-reference binding `{name}` cannot become ordinary local value-flow proof"
                        ),
                    )
                } else {
                    (
                        "unsupported_pointer_or_reference_value",
                        format!(
                            "C/C++ pointer/reference binding `{name}` cannot become ordinary local value-flow proof"
                        ),
                    )
                };
                self.push_gap(
                    gap_kind,
                    reason,
                    Some(source_span_for_node(&self.parsed.repo_relative_path, node)),
                    Some(analyses[analysis_index].scope.entity.id.clone()),
                );
                continue;
            }
            let binding = analyses[analysis_index]
                .bindings_by_name
                .get(&name)
                .and_then(|bindings| unique_binding(bindings))
                .cloned();
            if let Some(binding) = binding {
                self.push_bound_value_use_candidate(analysis_index, analyses, node, name, &binding);
            }
        }
    }

    fn add_derived_relations(
        &mut self,
        analyses: &[FunctionAnalysis<'a>],
        function_indices_by_name: &BTreeMap<String, Vec<usize>>,
    ) {
        for (analysis_index, analysis) in analyses.iter().enumerate() {
            for node in analysis.owned_nodes.iter().copied() {
                if let Some(parts) = assignment_parts(node) {
                    let Some(binding) = assignment_target_binding(
                        self.parsed.language,
                        analysis,
                        parts,
                        self.source,
                    )
                    .cloned() else {
                        continue;
                    };
                    let Some(assignment_id) = analysis
                        .candidate_by_ast_and_kind
                        .get(&(node.id(), MicroNodeKind::AssignmentSite))
                        .cloned()
                    else {
                        continue;
                    };
                    let relation_span = source_span_for_node(&self.parsed.repo_relative_path, node);

                    if let Some(right) = parts.right {
                        for producer in expression_result_sources(
                            self.parsed.language,
                            self.source,
                            right,
                            analysis_index,
                            analyses,
                            function_indices_by_name,
                        ) {
                            let fact_ids = deterministic_fact_ids(
                                [
                                    analysis.scope.entity.id.clone(),
                                    producer.candidate_id.clone(),
                                    assignment_id.clone(),
                                    binding.entity_id.clone(),
                                    role_tagged_fact_id(
                                        "producer_occurrence",
                                        &producer.candidate_id,
                                    ),
                                    role_tagged_fact_id("target_assignment", &assignment_id),
                                ]
                                .into_iter()
                                .chain(producer.exact_fact_ids),
                            );
                            self.push_edge(
                                MicroEdgeKind::LocalFlowsTo,
                                &producer.candidate_id,
                                &binding.candidate_id,
                                &analysis.scope.entity.id,
                                relation_span.clone(),
                                MicroDerivationKind::LocalAssignmentChainDerivation,
                                MicroExactness::DerivedWithProvenance,
                                fact_ids,
                            );
                        }
                    }

                    if binding.node_kind == MicroNodeKind::LocalBinding
                        && !assignment_is_binding_declaration(
                            self.parsed.language,
                            node,
                            parts,
                            &binding,
                            &self.parsed.repo_relative_path,
                        )
                    {
                        let Some(mutation_id) = analysis
                            .candidate_by_ast_and_kind
                            .get(&(node.id(), MicroNodeKind::MutationSite))
                            .cloned()
                        else {
                            continue;
                        };
                        self.push_edge(
                            MicroEdgeKind::LocalMutates,
                            &mutation_id,
                            &binding.candidate_id,
                            &analysis.scope.entity.id,
                            relation_span,
                            MicroDerivationKind::LocalSequenceDerivation,
                            MicroExactness::DerivedWithProvenance,
                            deterministic_fact_ids([
                                analysis.scope.entity.id.clone(),
                                mutation_id.clone(),
                                assignment_id.clone(),
                                binding.entity_id.clone(),
                                role_tagged_fact_id("producer_mutation", &mutation_id),
                                role_tagged_fact_id("target_assignment", &assignment_id),
                                role_tagged_fact_id("target_binding", &binding.entity_id),
                            ]),
                        );
                    }
                }

                if let Some(expression) = analysis.return_value_expression(node) {
                    let Some(return_id) = analysis
                        .candidate_by_ast_and_kind
                        .get(&(node.id(), MicroNodeKind::ReturnSite))
                        .cloned()
                    else {
                        continue;
                    };
                    let relation_span = source_span_for_node(&self.parsed.repo_relative_path, node);
                    for producer in expression_result_sources(
                        self.parsed.language,
                        self.source,
                        expression,
                        analysis_index,
                        analyses,
                        function_indices_by_name,
                    ) {
                        let fact_ids = deterministic_fact_ids(
                            [
                                analysis.scope.entity.id.clone(),
                                producer.candidate_id.clone(),
                                return_id.clone(),
                                role_tagged_fact_id("producer_occurrence", &producer.candidate_id),
                                role_tagged_fact_id("target_return", &return_id),
                            ]
                            .into_iter()
                            .chain(producer.exact_fact_ids),
                        );
                        self.push_edge(
                            MicroEdgeKind::LocalFlowsTo,
                            &producer.candidate_id,
                            &return_id,
                            &analysis.scope.entity.id,
                            relation_span.clone(),
                            MicroDerivationKind::LocalAssignmentChainDerivation,
                            MicroExactness::DerivedWithProvenance,
                            fact_ids,
                        );
                    }
                }
            }
            self.add_guard_relations(analysis);
        }
    }

    fn add_guard_relations(&mut self, analysis: &FunctionAnalysis<'a>) {
        let branch_arms = analysis
            .candidate_by_ast_and_kind
            .iter()
            .filter(|((_, kind), _)| *kind == MicroNodeKind::BranchArm)
            .filter_map(|((node_id, _), candidate_id)| {
                owned_node_by_id(analysis, *node_id).map(|node| (node, candidate_id.clone()))
            })
            .collect::<Vec<_>>();
        for ((operation_node_id, kind), operation_id) in &analysis.candidate_by_ast_and_kind {
            if !matches!(
                kind,
                MicroNodeKind::AssignmentSite
                    | MicroNodeKind::ReturnSite
                    | MicroNodeKind::CallSite
                    | MicroNodeKind::MutationSite
            ) {
                continue;
            }
            let Some(operation_node) = owned_node_by_id(analysis, *operation_node_id) else {
                continue;
            };
            let Some((_arm, arm_id)) = branch_arms
                .iter()
                .filter(|(arm, _)| node_contains(*arm, *operation_node_id))
                .min_by_key(|(arm, _)| (arm.end_byte() - arm.start_byte(), arm.start_byte()))
            else {
                continue;
            };
            self.push_edge(
                MicroEdgeKind::LocalGuards,
                arm_id,
                operation_id,
                &analysis.scope.entity.id,
                source_span_for_node(&self.parsed.repo_relative_path, operation_node),
                MicroDerivationKind::LocalSequenceDerivation,
                MicroExactness::DerivedWithProvenance,
                deterministic_fact_ids([
                    analysis.scope.entity.id.clone(),
                    arm_id.clone(),
                    operation_id.clone(),
                    role_tagged_fact_id("controlling_branch_arm", arm_id),
                    role_tagged_fact_id("guarded_operation", operation_id),
                    stable_fact_identity_key(
                        "parser_facts_v1_guard_relation",
                        [
                            self.parsed.repo_relative_path.as_str(),
                            analysis.scope.entity.id.as_str(),
                            arm_id.as_str(),
                            operation_id.as_str(),
                        ],
                    ),
                ]),
            );
        }
    }

    fn add_marker_relations(
        &mut self,
        analyses: &[FunctionAnalysis<'a>],
        function_indices_by_name: &BTreeMap<String, Vec<usize>>,
        marker_by_function_id: &BTreeMap<String, MarkerKind>,
    ) {
        for analysis in analyses {
            for node in analysis.owned_nodes.iter().copied() {
                if let Some(parts) = assignment_parts(node) {
                    let Some(call) = parts.right.and_then(|right| {
                        exact_single_call_expression(self.parsed.language, right)
                    }) else {
                        continue;
                    };
                    let Some(sanitizer_id) = analysis
                        .candidate_by_ast_and_kind
                        .get(&(call.id(), MicroNodeKind::SanitizerCall))
                        .cloned()
                    else {
                        continue;
                    };
                    let Some((target_index, MarkerKind::Sanitizer)) = resolve_direct_marked_target(
                        self.parsed.language,
                        call,
                        self.source,
                        analysis,
                        analyses,
                        function_indices_by_name,
                        marker_by_function_id,
                    ) else {
                        continue;
                    };
                    let input_ids = candidate_ids_within(analysis, call, MicroNodeKind::ValueUse);
                    if input_ids.len() != 1 {
                        self.push_gap(
                            "sanitizer_input_not_unique",
                            format!(
                                "marked sanitizer call has {} exact input value occurrences; exactly one is required",
                                input_ids.len()
                            ),
                            Some(source_span_for_node(
                                &self.parsed.repo_relative_path,
                                call,
                            )),
                            Some(analysis.scope.entity.id.clone()),
                        );
                        continue;
                    }
                    let input_id = input_ids[0].clone();
                    let Some(input_node) = self.node_by_id(&input_id).cloned() else {
                        continue;
                    };
                    let Some(input_binding_id) = input_node.symbol_binding_id.as_deref() else {
                        continue;
                    };
                    let Some(input_binding) = analysis
                        .bindings_by_name
                        .values()
                        .flatten()
                        .find(|binding| binding.entity_id == input_binding_id)
                        .cloned()
                    else {
                        continue;
                    };
                    let Some(output_binding) = assignment_target_binding(
                        self.parsed.language,
                        analysis,
                        parts,
                        self.source,
                    )
                    .cloned() else {
                        self.push_gap(
                            "sanitizer_output_not_unique",
                            "marked sanitizer result is not assigned to one exact local binding",
                            Some(source_span_for_node(&self.parsed.repo_relative_path, node)),
                            Some(analysis.scope.entity.id.clone()),
                        );
                        continue;
                    };
                    let Some(assignment_id) = analysis
                        .candidate_by_ast_and_kind
                        .get(&(node.id(), MicroNodeKind::AssignmentSite))
                        .cloned()
                    else {
                        continue;
                    };
                    let Some(callsite_id) = analysis
                        .candidate_by_ast_and_kind
                        .get(&(call.id(), MicroNodeKind::CallSite))
                        .cloned()
                    else {
                        continue;
                    };
                    let helper_id = analyses[target_index].scope.entity.id.clone();
                    let helper_frame_id = analyses[target_index].scope.frame_id.clone();
                    let relation_span = source_span_for_node(&self.parsed.repo_relative_path, call);
                    let shared_fact_ids = [
                        analysis.scope.entity.id.clone(),
                        input_id.clone(),
                        input_binding.entity_id.clone(),
                        sanitizer_id.clone(),
                        callsite_id.clone(),
                        assignment_id.clone(),
                        output_binding.entity_id.clone(),
                        helper_id.clone(),
                        helper_frame_id,
                        role_tagged_fact_id("codegraph_sanitizer_marker", &helper_id),
                        role_tagged_fact_id("marked_sanitizer_helper", &helper_id),
                        role_tagged_fact_id("sanitizer_callsite", &callsite_id),
                        role_tagged_fact_id("sanitizer_input_occurrence", &input_id),
                        role_tagged_fact_id("sanitizer_input_binding", &input_binding.entity_id),
                        role_tagged_fact_id("sanitizer_call", &sanitizer_id),
                        role_tagged_fact_id("target_assignment", &assignment_id),
                        role_tagged_fact_id("sanitized_output_binding", &output_binding.entity_id),
                    ];
                    self.push_edge(
                        MicroEdgeKind::LocalFlowsTo,
                        &input_id,
                        &sanitizer_id,
                        &analysis.scope.entity.id,
                        relation_span.clone(),
                        MicroDerivationKind::LocalAssignmentChainDerivation,
                        MicroExactness::DerivedWithProvenance,
                        deterministic_fact_ids(shared_fact_ids.clone()),
                    );
                    self.push_edge(
                        MicroEdgeKind::LocalSanitizes,
                        &input_binding.candidate_id,
                        &output_binding.candidate_id,
                        &analysis.scope.entity.id,
                        relation_span.clone(),
                        MicroDerivationKind::LocalAssignmentChainDerivation,
                        MicroExactness::DerivedWithProvenance,
                        deterministic_fact_ids(shared_fact_ids.clone()),
                    );
                    self.push_edge(
                        MicroEdgeKind::LocalFlowsTo,
                        &sanitizer_id,
                        &output_binding.candidate_id,
                        &analysis.scope.entity.id,
                        relation_span,
                        MicroDerivationKind::LocalAssignmentChainDerivation,
                        MicroExactness::DerivedWithProvenance,
                        deterministic_fact_ids(shared_fact_ids),
                    );
                }

                let Some(assertion_id) = analysis
                    .candidate_by_ast_and_kind
                    .get(&(node.id(), MicroNodeKind::TestAssertion))
                    .cloned()
                else {
                    continue;
                };
                let (input_ids, helper_fact_ids, assertion_role) = if is_native_assertion_node(
                    self.parsed.language,
                    node,
                ) {
                    (
                        candidate_ids_within(analysis, node, MicroNodeKind::ValueUse),
                        Vec::new(),
                        "native_assertion_syntax",
                    )
                } else if is_call_node(self.parsed.language, node) {
                    let Some((target_index, MarkerKind::Assertion)) = resolve_direct_marked_target(
                        self.parsed.language,
                        node,
                        self.source,
                        analysis,
                        analyses,
                        function_indices_by_name,
                        marker_by_function_id,
                    ) else {
                        continue;
                    };
                    let input_ids = candidate_ids_within(analysis, node, MicroNodeKind::ValueUse);
                    if input_ids.len() != 1 {
                        self.push_gap(
                                "assertion_input_not_unique",
                                format!(
                                    "marked assertion call has {} exact input value occurrences; exactly one is required",
                                    input_ids.len()
                                ),
                                Some(source_span_for_node(
                                    &self.parsed.repo_relative_path,
                                    node,
                                )),
                                Some(analysis.scope.entity.id.clone()),
                            );
                        continue;
                    }
                    let callsite_id = analysis
                        .candidate_by_ast_and_kind
                        .get(&(node.id(), MicroNodeKind::CallSite))
                        .cloned()
                        .into_iter()
                        .collect::<Vec<_>>();
                    let helper_id = analyses[target_index].scope.entity.id.clone();
                    (
                        input_ids,
                        callsite_id
                            .into_iter()
                            .chain(std::iter::once(helper_id))
                            .collect(),
                        "marked_assertion_helper",
                    )
                } else {
                    continue;
                };
                for input_id in input_ids {
                    let Some(input_node) = self.node_by_id(&input_id).cloned() else {
                        continue;
                    };
                    let Some(binding_id) = input_node.symbol_binding_id.clone() else {
                        continue;
                    };
                    let fact_ids = deterministic_fact_ids(
                        [
                            analysis.scope.entity.id.clone(),
                            input_id.clone(),
                            binding_id,
                            assertion_id.clone(),
                            role_tagged_fact_id("asserted_value_occurrence", &input_id),
                            role_tagged_fact_id("assertion_site", &assertion_id),
                            role_tagged_fact_id(assertion_role, &assertion_id),
                        ]
                        .into_iter()
                        .chain(helper_fact_ids.clone()),
                    );
                    self.push_edge(
                        MicroEdgeKind::LocalAsserts,
                        &input_id,
                        &assertion_id,
                        &analysis.scope.entity.id,
                        source_span_for_node(&self.parsed.repo_relative_path, node),
                        MicroDerivationKind::LocalAssignmentChainDerivation,
                        MicroExactness::DerivedWithProvenance,
                        fact_ids,
                    );
                }
            }
        }
    }

    fn push_bound_value_use_candidate(
        &mut self,
        analysis_index: usize,
        analyses: &mut [FunctionAnalysis<'a>],
        node: Node<'a>,
        name: String,
        binding: &BindingRef,
    ) -> String {
        if let Some(existing) = analyses[analysis_index]
            .candidate_by_ast_and_kind
            .get(&(node.id(), MicroNodeKind::ValueUse))
        {
            return existing.clone();
        }
        let scope = &analyses[analysis_index].scope;
        let candidate = self.make_candidate(
            MicroNodeKind::ValueUse,
            &scope.entity.id,
            &name,
            source_span_for_node(&self.parsed.repo_relative_path, node),
            Some(scope.entity.id.clone()),
            Some(binding.entity_id.clone()),
            scope.scope_path.clone(),
            structural_path_to_node(self.parsed.tree().root_node(), node),
            vec![binding.entity_id.clone()],
        );
        let candidate_id = candidate.micro_node_id.clone();
        self.push_node(candidate);
        analyses[analysis_index]
            .candidate_by_ast_and_kind
            .insert((node.id(), MicroNodeKind::ValueUse), candidate_id.clone());
        candidate_id
    }

    fn add_exact_relations(
        &mut self,
        analyses: &[FunctionAnalysis<'a>],
        function_indices_by_name: &BTreeMap<String, Vec<usize>>,
    ) {
        for (analysis_index, analysis) in analyses.iter().enumerate() {
            let value_uses = analysis
                .candidate_by_ast_and_kind
                .iter()
                .filter(|((_, kind), _)| *kind == MicroNodeKind::ValueUse)
                .map(|(_, candidate_id)| candidate_id.clone())
                .collect::<Vec<_>>();
            for value_use_id in value_uses {
                let Some(value_use) = self.node_by_id(&value_use_id).cloned() else {
                    continue;
                };
                let Some(binding_id) = value_use.symbol_binding_id.as_deref() else {
                    continue;
                };
                let Some(binding) = analysis
                    .bindings_by_name
                    .values()
                    .flatten()
                    .find(|binding| binding.entity_id == binding_id)
                    .cloned()
                else {
                    continue;
                };
                self.push_edge(
                    MicroEdgeKind::LocalReads,
                    &value_use_id,
                    &binding.candidate_id,
                    &analysis.scope.entity.id,
                    value_use.source_span,
                    MicroDerivationKind::ResolverBackedBinding,
                    MicroExactness::Exact,
                    vec![binding.entity_id],
                );
            }

            let nodes = analysis.owned_nodes.clone();
            for node in nodes {
                if let Some(parts) = assignment_parts(node) {
                    let Some(assignment_id) = analysis
                        .candidate_by_ast_and_kind
                        .get(&(node.id(), MicroNodeKind::AssignmentSite))
                        .cloned()
                    else {
                        continue;
                    };
                    let Some(binding) = assignment_target_binding(
                        self.parsed.language,
                        analysis,
                        parts,
                        self.source,
                    )
                    .cloned() else {
                        continue;
                    };
                    self.push_edge(
                        MicroEdgeKind::LocalWrites,
                        &assignment_id,
                        &binding.candidate_id,
                        &analysis.scope.entity.id,
                        source_span_for_node(&self.parsed.repo_relative_path, node),
                        MicroDerivationKind::ResolverBackedBinding,
                        MicroExactness::Exact,
                        vec![binding.entity_id],
                    );
                }

                if analysis.is_return_site(node) {
                    if let Some(return_id) = analysis
                        .candidate_by_ast_and_kind
                        .get(&(node.id(), MicroNodeKind::ReturnSite))
                        .cloned()
                    {
                        self.push_edge(
                            MicroEdgeKind::LocalReturnsTo,
                            &return_id,
                            &analysis.scope.frame_id,
                            &analysis.scope.entity.id,
                            source_span_for_node(&self.parsed.repo_relative_path, node),
                            MicroDerivationKind::DirectAstExtraction,
                            MicroExactness::Exact,
                            Vec::new(),
                        );
                    }
                }

                if is_call_node(self.parsed.language, node) {
                    let Some(callsite_id) = analysis
                        .candidate_by_ast_and_kind
                        .get(&(node.id(), MicroNodeKind::CallSite))
                        .cloned()
                    else {
                        continue;
                    };
                    match resolve_direct_target(
                        self.parsed.language,
                        node,
                        self.source,
                        analysis_index,
                        analyses,
                        function_indices_by_name,
                    ) {
                        Ok(target_index) => {
                            let target = &analyses[target_index].scope;
                            self.push_edge(
                                MicroEdgeKind::LocalCalls,
                                &callsite_id,
                                &target.frame_id,
                                &analysis.scope.entity.id,
                                source_span_for_node(&self.parsed.repo_relative_path, node),
                                MicroDerivationKind::ResolverBackedBinding,
                                MicroExactness::Exact,
                                vec![target.entity.id.clone()],
                            );
                        }
                        Err(reason) => self.push_gap(
                            "unresolved_direct_call",
                            reason,
                            Some(source_span_for_node(&self.parsed.repo_relative_path, node)),
                            Some(analysis.scope.entity.id.clone()),
                        ),
                    }
                }

                if is_condition_node(node) {
                    let condition = condition_expression(node).unwrap_or(node);
                    let Some(condition_id) = analysis
                        .candidate_by_ast_and_kind
                        .get(&(condition.id(), MicroNodeKind::ConditionSite))
                        .cloned()
                    else {
                        continue;
                    };
                    for value_use_id in
                        candidate_ids_within(analysis, condition, MicroNodeKind::ValueUse)
                    {
                        self.push_edge(
                            MicroEdgeKind::LocalChecks,
                            &condition_id,
                            &value_use_id,
                            &analysis.scope.entity.id,
                            source_span_for_node(&self.parsed.repo_relative_path, condition),
                            MicroDerivationKind::DirectAstExtraction,
                            MicroExactness::Exact,
                            Vec::new(),
                        );
                    }
                    for (_, arm) in condition_branch_arms(node, condition) {
                        let Some(arm_id) = analysis
                            .candidate_by_ast_and_kind
                            .get(&(arm.id(), MicroNodeKind::BranchArm))
                            .cloned()
                        else {
                            continue;
                        };
                        self.push_edge(
                            MicroEdgeKind::LocalBranchesTo,
                            &condition_id,
                            &arm_id,
                            &analysis.scope.entity.id,
                            source_span_for_node(&self.parsed.repo_relative_path, arm),
                            MicroDerivationKind::DirectAstExtraction,
                            MicroExactness::Exact,
                            Vec::new(),
                        );
                    }
                }
            }
        }
    }

    fn push_ast_candidate(
        &mut self,
        analysis_index: usize,
        analyses: &mut [FunctionAnalysis<'a>],
        kind: MicroNodeKind,
        node: Node<'a>,
        name: String,
    ) -> String {
        if let Some(existing) = analyses[analysis_index]
            .candidate_by_ast_and_kind
            .get(&(node.id(), kind))
        {
            return existing.clone();
        }
        let scope = &analyses[analysis_index].scope;
        let candidate = self.make_candidate(
            kind,
            &scope.entity.id,
            &name,
            source_span_for_node(&self.parsed.repo_relative_path, node),
            Some(scope.entity.id.clone()),
            None,
            scope.scope_path.clone(),
            structural_path_to_node(self.parsed.tree().root_node(), node),
            Vec::new(),
        );
        let candidate_id = candidate.micro_node_id.clone();
        self.push_node(candidate);
        analyses[analysis_index]
            .candidate_by_ast_and_kind
            .insert((node.id(), kind), candidate_id.clone());
        candidate_id
    }

    #[allow(clippy::too_many_arguments)]
    fn make_candidate(
        &self,
        kind: MicroNodeKind,
        function_identity: &str,
        name: &str,
        source_span: SourceSpan,
        function_entity_id: Option<String>,
        binding_id: Option<String>,
        scope_path: Vec<String>,
        structural_path: Vec<String>,
        source_fact_ids: Vec<String>,
    ) -> Mvp4MicroNodeCandidate {
        let identity = MicroNodeIdentityInput {
            repo_relative_path: self.parsed.repo_relative_path.clone(),
            language: self.bundle.file_identity.language.clone(),
            kind,
            function_entity_id: function_entity_id.clone(),
            scope_path: scope_path.clone(),
            binding_id: binding_id.clone(),
            symbol: Some(name.to_string()),
            structural_path: structural_path.clone(),
            occurrence_index: 0,
            source_span: Some(source_span.clone()),
            parse_recovery: false,
        };
        Mvp4MicroNodeCandidate {
            micro_node_id: stable_micro_node_id(&identity),
            node_kind: kind,
            repo_relative_path: self.parsed.repo_relative_path.clone(),
            language: self.bundle.file_identity.language.clone(),
            frontend: self.report.frontend.clone(),
            enclosing_function_identity: function_identity.to_string(),
            function_entity_id,
            function_kind: None,
            scope_path,
            structural_ast_path: structural_path,
            occurrence_index: 0,
            source_span: source_span.clone(),
            name_or_literal: bounded_label(name),
            symbol_binding_id: binding_id,
            source_role: self.source_role,
            exactness: MicroExactness::Exact,
            claimability: PARSER_FACTS_V1_CLAIMABILITY.to_string(),
            parse_recovery: false,
            declaration_kind: None,
            row_schema_version: PARSER_FACTS_V1_ROW_SCHEMA_VERSION,
            payload_version: PARSER_FACTS_V1_PAYLOAD_VERSION,
            extraction_version: PARSER_FACTS_V1_EXTRACTION_VERSION.to_string(),
            provenance: MicroFactProvenance {
                derivation_kind: MicroDerivationKind::DirectAstExtraction,
                source_fact_ids,
                source_spans: vec![source_span],
                extractor_or_adapter_version: PARSER_FACTS_V1_EXTRACTION_VERSION.to_string(),
                exactness: MicroExactness::Exact,
                limitations: vec!["parser_facts_v1_is_not_production_activated".to_string()],
            },
        }
    }

    fn push_node(&mut self, candidate: Mvp4MicroNodeCandidate) {
        if self.node_ids.insert(candidate.micro_node_id.clone()) {
            self.report.nodes.push(candidate);
        }
    }

    fn node_by_id(&self, candidate_id: &str) -> Option<&Mvp4MicroNodeCandidate> {
        self.report
            .nodes
            .iter()
            .find(|candidate| candidate.micro_node_id == candidate_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_edge(
        &mut self,
        kind: MicroEdgeKind,
        head_id: &str,
        tail_id: &str,
        function_identity: &str,
        relation_source_span: SourceSpan,
        derivation_kind: MicroDerivationKind,
        exactness: MicroExactness,
        source_fact_ids: Vec<String>,
    ) {
        let Some(head) = self.node_by_id(head_id).cloned() else {
            return;
        };
        let Some(tail) = self.node_by_id(tail_id).cloned() else {
            return;
        };
        let identity = MicroEdgeIdentityInput {
            repo_relative_path: self.parsed.repo_relative_path.clone(),
            language: self.bundle.file_identity.language.clone(),
            kind,
            function_entity_id: Some(function_identity.to_string()),
            scope_path: head.scope_path.clone(),
            source_micro_node_id: head_id.to_string(),
            target_micro_node_id: tail_id.to_string(),
            structural_path: vec![head_id.to_string(), tail_id.to_string()],
            occurrence_index: 0,
            source_span: Some(relation_source_span.clone()),
        };
        let micro_edge_id = stable_micro_edge_id(&identity);
        if !self.edge_ids.insert(micro_edge_id.clone()) {
            return;
        }
        let mut source_spans = Vec::from([
            relation_source_span.clone(),
            head.source_span.clone(),
            tail.source_span.clone(),
        ]);
        for fact_id in &source_fact_ids {
            if let Some(node) = self.node_by_id(fact_id) {
                source_spans.push(node.source_span.clone());
            }
        }
        self.report.edges.push(MicroEdgeCandidate {
            micro_edge_id,
            micro_edge_kind: kind,
            head_micro_node_id: head_id.to_string(),
            tail_micro_node_id: tail_id.to_string(),
            repo_relative_path: self.parsed.repo_relative_path.clone(),
            function_identity: function_identity.to_string(),
            relation_source_span: relation_source_span.clone(),
            head_source_span: head.source_span,
            tail_source_span: tail.source_span,
            provenance: MicroFactProvenance {
                derivation_kind,
                source_fact_ids,
                source_spans,
                extractor_or_adapter_version: PARSER_FACTS_V1_EXTRACTION_VERSION.to_string(),
                exactness,
                limitations: vec!["parser_facts_v1_is_not_production_activated".to_string()],
            },
            exactness,
            claimability: PARSER_FACTS_V1_CLAIMABILITY.to_string(),
            language: self.bundle.file_identity.language.clone(),
            frontend: self.report.frontend.clone(),
            source_role: self.source_role,
            row_schema_version: PARSER_FACTS_V1_ROW_SCHEMA_VERSION,
            payload_version: PARSER_FACTS_V1_PAYLOAD_VERSION,
            extraction_version: PARSER_FACTS_V1_EXTRACTION_VERSION.to_string(),
            cap_omission_state: None,
        });
    }

    fn push_gap(
        &mut self,
        gap_kind: impl Into<String>,
        reason: impl Into<String>,
        source_span: Option<SourceSpan>,
        function_identity: Option<String>,
    ) {
        self.report.gaps.push(ParserFactsV1Gap {
            gap_kind: gap_kind.into(),
            reason: reason.into(),
            source_span,
            function_identity,
        });
    }
}

fn parser_fact_is_exact(exactness: Exactness) -> bool {
    matches!(
        exactness,
        Exactness::ParserVerified | Exactness::CompilerVerified | Exactness::LspVerified
    )
}

fn is_function_entity_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Function | EntityKind::Method | EntityKind::Constructor
    )
}

fn bundle_parent_chain(bundle: &ParserFactBundle) -> (BTreeMap<String, String>, BTreeSet<String>) {
    let mut parent_by_child = BTreeMap::new();
    let mut ambiguous = BTreeSet::new();
    for fact in &bundle.relation_facts {
        if fact.edge.relation != codegraph_core::RelationKind::Contains
            || !parser_fact_is_exact(fact.exactness)
        {
            continue;
        }
        match parent_by_child.get(&fact.edge.tail_id) {
            Some(existing) if existing != &fact.edge.head_id => {
                ambiguous.insert(fact.edge.tail_id.clone());
            }
            None => {
                parent_by_child.insert(fact.edge.tail_id.clone(), fact.edge.head_id.clone());
            }
            _ => {}
        }
    }
    for child in &ambiguous {
        parent_by_child.remove(child);
    }
    (parent_by_child, ambiguous)
}

fn bundle_scope_path(
    entity_id: &str,
    parent_by_child: &BTreeMap<String, String>,
    entities_by_id: &BTreeMap<String, Entity>,
) -> Vec<String> {
    let mut path = Vec::new();
    let mut seen = BTreeSet::new();
    let mut current = parent_by_child.get(entity_id).map(String::as_str);
    while let Some(id) = current {
        if !seen.insert(id.to_string()) {
            break;
        }
        if let Some(entity) = entities_by_id.get(id) {
            path.push(format!("{}:{}", entity.kind.id_prefix(), entity.name));
        }
        current = parent_by_child.get(id).map(String::as_str);
    }
    path.reverse();
    path
}

fn nearest_function_ancestor(
    entity_id: &str,
    parent_by_child: &BTreeMap<String, String>,
    function_ids: &BTreeSet<String>,
) -> Option<String> {
    let mut seen = BTreeSet::new();
    let mut current = parent_by_child.get(entity_id).map(String::as_str);
    while let Some(id) = current {
        if !seen.insert(id.to_string()) {
            return None;
        }
        if function_ids.contains(id) {
            return Some(id.to_string());
        }
        current = parent_by_child.get(id).map(String::as_str);
    }
    None
}

fn collect_function_nodes<'tree>(
    node: Node<'tree>,
    language: SourceLanguage,
    functions: &mut Vec<Node<'tree>>,
) {
    if generic_decl_kind(language, node).is_some_and(is_function_entity_kind) {
        functions.push(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_function_nodes(child, language, functions);
    }
}

fn function_body_node(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("body").or_else(|| {
        let mut cursor = node.walk();
        let body = node.named_children(&mut cursor).find(|child| {
            matches!(
                child.kind(),
                "statement_block"
                    | "block"
                    | "compound_statement"
                    | "body_statement"
                    | "declaration_list"
            )
        });
        body
    })
}

#[derive(Debug)]
enum RustTailOutcome<'tree> {
    Values(Vec<Node<'tree>>),
    Diverges,
    UnitOrUnsupported,
}

fn language_implicit_return_expressions<'tree>(
    language: SourceLanguage,
    function: Node<'tree>,
    body: Node<'tree>,
    source: &str,
) -> Vec<Node<'tree>> {
    match language {
        SourceLanguage::Rust => rust_implicit_return_expressions(language, function, body, source),
        SourceLanguage::Ruby if !node_has_error_or_missing_descendant(body) => {
            ruby_implicit_return_tail(body)
                .filter(|tail| ruby_implicit_return_tail_is_safe(*tail, source))
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    }
}

fn ruby_implicit_return_tail(body: Node<'_>) -> Option<Node<'_>> {
    if body.kind() != "body_statement" {
        return Some(body);
    }
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|child| !child.kind().contains("comment"))
        .last()
}

fn ruby_implicit_return_tail_is_safe(tail: Node<'_>, source: &str) -> bool {
    if tail.kind() == "identifier" {
        return true;
    }
    if matches!(
        tail.kind(),
        "integer"
            | "float"
            | "rational"
            | "complex"
            | "true"
            | "false"
            | "nil"
            | "simple_symbol"
            | "symbol"
    ) {
        return true;
    }
    if tail.kind() == "string" {
        return !contains_named_kind(tail, "interpolation");
    }
    ruby_plain_unqualified_call_name(tail, source).is_some()
}

fn rust_implicit_return_expressions<'tree>(
    language: SourceLanguage,
    function: Node<'tree>,
    body: Node<'tree>,
    source: &str,
) -> Vec<Node<'tree>> {
    if language != SourceLanguage::Rust || node_has_error_or_missing_descendant(body) {
        return Vec::new();
    }
    let Some(return_type) = function.child_by_field_name("return_type") else {
        return Vec::new();
    };
    let Some(return_type_text) = node_text(return_type, source) else {
        return Vec::new();
    };
    if matches!(return_type_text.trim(), "()" | "!") {
        return Vec::new();
    }

    let RustTailOutcome::Values(mut expressions) = rust_tail_outcome(body, source) else {
        return Vec::new();
    };
    expressions.sort_by_key(|expression| (expression.start_byte(), expression.end_byte()));
    expressions.dedup_by_key(|expression| expression.id());
    expressions
}

fn rust_tail_outcome<'tree>(node: Node<'tree>, source: &str) -> RustTailOutcome<'tree> {
    if node_has_error_or_missing_descendant(node) {
        return RustTailOutcome::UnitOrUnsupported;
    }
    if is_return_node(node) {
        return RustTailOutcome::Diverges;
    }

    match node.kind() {
        "block" => rust_last_semantic_named_child(node)
            .map(|tail| rust_tail_outcome(tail, source))
            .unwrap_or(RustTailOutcome::UnitOrUnsupported),
        "expression_statement" => {
            let Some(expression) = rust_last_semantic_named_child(node) else {
                return RustTailOutcome::UnitOrUnsupported;
            };
            if is_return_node(expression) {
                return RustTailOutcome::Diverges;
            }
            if node_text(node, source).is_some_and(|text| text.trim_end().ends_with(';')) {
                RustTailOutcome::UnitOrUnsupported
            } else {
                rust_tail_outcome(expression, source)
            }
        }
        "if_expression" => {
            let Some(consequence) = node.child_by_field_name("consequence") else {
                return RustTailOutcome::UnitOrUnsupported;
            };
            let Some(alternative) = node.child_by_field_name("alternative") else {
                return RustTailOutcome::UnitOrUnsupported;
            };
            merge_rust_branch_outcomes([
                rust_tail_outcome(consequence, source),
                rust_tail_outcome(alternative, source),
            ])
        }
        "else_clause" | "match_arm" => rust_last_semantic_named_child(node)
            .map(|tail| rust_tail_outcome(tail, source))
            .unwrap_or(RustTailOutcome::UnitOrUnsupported),
        "match_expression" => {
            let Some(body) = node.child_by_field_name("body") else {
                return RustTailOutcome::UnitOrUnsupported;
            };
            let mut cursor = body.walk();
            let outcomes = body
                .named_children(&mut cursor)
                .filter(|arm| arm.kind() == "match_arm")
                .map(|arm| {
                    arm.child_by_field_name("value")
                        .map(|value| rust_tail_outcome(value, source))
                        .unwrap_or(RustTailOutcome::UnitOrUnsupported)
                })
                .collect::<Vec<_>>();
            merge_rust_branch_outcomes(outcomes)
        }
        "let_declaration"
        | "const_item"
        | "static_item"
        | "type_item"
        | "use_declaration"
        | "mod_item"
        | "function_item"
        | "impl_item"
        | "trait_item"
        | "struct_item"
        | "enum_item"
        | "macro_definition"
        | "empty_statement"
        | "loop_expression"
        | "while_expression"
        | "for_expression"
        | "break_expression"
        | "continue_expression" => RustTailOutcome::UnitOrUnsupported,
        _ if node_text(node, source).is_some_and(|text| text.trim() == "()") => {
            RustTailOutcome::UnitOrUnsupported
        }
        _ => RustTailOutcome::Values(vec![node]),
    }
}

fn merge_rust_branch_outcomes<'tree>(
    outcomes: impl IntoIterator<Item = RustTailOutcome<'tree>>,
) -> RustTailOutcome<'tree> {
    let mut values = Vec::new();
    let mut saw_branch = false;
    for outcome in outcomes {
        saw_branch = true;
        match outcome {
            RustTailOutcome::Values(mut branch_values) => values.append(&mut branch_values),
            RustTailOutcome::Diverges => {}
            RustTailOutcome::UnitOrUnsupported => return RustTailOutcome::UnitOrUnsupported,
        }
    }
    if !saw_branch {
        RustTailOutcome::UnitOrUnsupported
    } else if values.is_empty() {
        RustTailOutcome::Diverges
    } else {
        RustTailOutcome::Values(values)
    }
}

fn rust_last_semantic_named_child(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
        .last()
}

fn find_parameter_container(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(parameters) = node.child_by_field_name("parameters") {
        return Some(parameters);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if matches!(
            child.kind(),
            "statement_block" | "block" | "compound_statement" | "body_statement"
        ) {
            continue;
        }
        if let Some(parameters) = find_parameter_container(child) {
            return Some(parameters);
        }
    }
    None
}

fn contains_named_kind(node: Node<'_>, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .any(|child| contains_named_kind(child, kind));
    found
}

fn collect_identifier_nodes<'tree>(node: Node<'tree>, identifiers: &mut Vec<Node<'tree>>) {
    if is_identifier_node(node) {
        identifiers.push(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_identifier_nodes(child, identifiers);
    }
}

fn collect_owned_function_nodes<'tree>(
    node: Node<'tree>,
    owner_function_id: usize,
    language: SourceLanguage,
    nodes: &mut Vec<Node<'tree>>,
) {
    if node.id() != owner_function_id && language_owned_semantic_boundary(language, node) {
        nodes.push(node);
        return;
    }
    if node.id() != owner_function_id
        && generic_decl_kind(language, node).is_some_and(is_function_entity_kind)
    {
        return;
    }
    nodes.push(node);
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_owned_function_nodes(child, owner_function_id, language, nodes);
    }
}

fn find_smallest_exact_span_node<'tree>(
    node: Node<'tree>,
    repo_relative_path: &str,
    span: &SourceSpan,
) -> Option<Node<'tree>> {
    let mut best = (source_span_for_node(repo_relative_path, node) == *span).then_some(node);
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(found) = find_smallest_exact_span_node(child, repo_relative_path, span) {
            best = Some(found);
        }
    }
    best
}

fn comment_nodes(root: Node<'_>) -> Vec<Node<'_>> {
    fn collect<'tree>(node: Node<'tree>, comments: &mut Vec<Node<'tree>>) {
        if node.kind().contains("comment") {
            comments.push(node);
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            collect(child, comments);
        }
    }
    let mut comments = Vec::new();
    collect(root, &mut comments);
    comments.sort_by_key(|node| node.end_byte());
    comments
}

fn marker_for_function(
    function: Node<'_>,
    comments: &[Node<'_>],
    source: &str,
) -> Option<MarkerKind> {
    let comment = comments
        .iter()
        .copied()
        .filter(|comment| comment.end_byte() <= function.start_byte())
        .max_by_key(|comment| comment.end_byte())?;
    let between = source.get(comment.end_byte()..function.start_byte())?;
    if !between.trim().is_empty() || between.bytes().filter(|byte| *byte == b'\n').count() > 1 {
        return None;
    }
    let text = node_text(comment, source)?;
    let marker = text
        .trim()
        .trim_start_matches("//")
        .trim_start_matches("/*")
        .trim_start_matches('#')
        .trim_end_matches("*/")
        .trim();
    match marker {
        "@codegraph-sanitizer" => Some(MarkerKind::Sanitizer),
        "@codegraph-assertion" => Some(MarkerKind::Assertion),
        _ => None,
    }
}

fn function_indices_by_name(
    language: SourceLanguage,
    analyses: &[FunctionAnalysis<'_>],
    source: &str,
) -> BTreeMap<String, Vec<usize>> {
    let mut by_name = BTreeMap::new();
    for (index, analysis) in analyses.iter().enumerate() {
        by_name
            .entry(function_declaration_resolution_key(
                language,
                &analysis.scope,
                source,
            ))
            .or_insert_with(Vec::new)
            .push(index);
    }
    by_name
}

fn function_declaration_resolution_key(
    language: SourceLanguage,
    scope: &FunctionScope<'_>,
    source: &str,
) -> String {
    let declared_name = if language == SourceLanguage::Php {
        scope
            .ast_node
            .child_by_field_name("name")
            .and_then(|name| node_text(name, source))
            .unwrap_or_else(|| scope.entity.name.clone())
    } else {
        scope.entity.name.clone()
    };
    function_resolution_key(language, scope, &declared_name, source)
}

fn function_resolution_key(
    language: SourceLanguage,
    scope: &FunctionScope<'_>,
    raw_name: &str,
    source: &str,
) -> String {
    if language != SourceLanguage::Php {
        return normalize_binding_name(raw_name);
    }
    format!(
        "php:{}::{}",
        php_namespace_domain(scope.ast_node, source),
        normalize_binding_name(raw_name).to_lowercase()
    )
}

fn php_namespace_domain(function: Node<'_>, source: &str) -> String {
    let mut current = function.parent();
    while let Some(parent) = current {
        if parent.kind() == "namespace_definition" {
            return php_namespace_name(parent, source);
        }
        current = parent.parent();
    }

    if function
        .parent()
        .is_some_and(|parent| parent.kind() == "program")
    {
        let mut sibling = function.prev_named_sibling();
        while let Some(previous) = sibling {
            if previous.kind() == "namespace_definition" {
                if previous.child_by_field_name("body").is_none() {
                    return php_namespace_name(previous, source);
                }
                break;
            }
            sibling = previous.prev_named_sibling();
        }
    }
    "global".to_string()
}

fn php_namespace_name(namespace: Node<'_>, source: &str) -> String {
    let Some(raw_name) = namespace
        .child_by_field_name("name")
        .and_then(|name| node_text(name, source))
    else {
        return "global".to_string();
    };
    let normalized = raw_name
        .trim()
        .trim_matches('\\')
        .split('\\')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_lowercase())
        .collect::<Vec<_>>()
        .join("\\");
    if normalized.is_empty() {
        "global".to_string()
    } else {
        normalized
    }
}

fn unique_binding(bindings: &[BindingRef]) -> Option<&BindingRef> {
    let unique_ids = bindings
        .iter()
        .map(|binding| binding.entity_id.as_str())
        .collect::<BTreeSet<_>>();
    (unique_ids.len() == 1).then(|| &bindings[0])
}

fn deterministic_fact_ids(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    ids.into_iter()
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn role_tagged_fact_id(role: &str, fact_id: &str) -> String {
    format!(
        "parser_facts_v1_role:{role}:{}",
        stable_fact_identity_key("parser_facts_v1_provenance_role", [role, fact_id],)
    )
}

fn owned_node_by_id<'tree>(
    analysis: &FunctionAnalysis<'tree>,
    node_id: usize,
) -> Option<Node<'tree>> {
    analysis
        .owned_nodes
        .iter()
        .copied()
        .find(|node| node.id() == node_id)
}

fn assignment_target_binding<'analysis>(
    language: SourceLanguage,
    analysis: &'analysis FunctionAnalysis<'_>,
    parts: AssignmentParts<'_>,
    source: &str,
) -> Option<&'analysis BindingRef> {
    if is_property_access_node(language, parts.left)
        || c_family_indirect_lvalue(language, parts.left)
        || language_assignment_target_is_unsupported(language, parts.left)
    {
        return None;
    }
    let name = simple_binding_name(parts.left, source)?;
    if analysis.non_value_flow_bindings.contains(&name) {
        return None;
    }
    analysis
        .bindings_by_name
        .get(&name)
        .and_then(|bindings| unique_binding(bindings))
}

fn assignment_is_binding_declaration(
    language: SourceLanguage,
    node: Node<'_>,
    parts: AssignmentParts<'_>,
    binding: &BindingRef,
    repo_relative_path: &str,
) -> bool {
    is_declaration_assignment(node)
        || (is_implicit_local_declaration_node(language, node)
            && source_span_for_node(repo_relative_path, parts.left) == binding.declaration_span)
}

fn expression_result_sources<'tree>(
    language: SourceLanguage,
    source: &str,
    expression: Node<'tree>,
    analysis_index: usize,
    analyses: &[FunctionAnalysis<'tree>],
    function_indices_by_name: &BTreeMap<String, Vec<usize>>,
) -> Vec<ExpressionResultSource> {
    fn collect<'tree>(
        language: SourceLanguage,
        source: &str,
        node: Node<'tree>,
        analysis_index: usize,
        analyses: &[FunctionAnalysis<'tree>],
        function_indices_by_name: &BTreeMap<String, Vec<usize>>,
        sources: &mut Vec<ExpressionResultSource>,
    ) {
        if language_expression_result_is_unsupported(language, node, source) {
            return;
        }
        if is_property_access_node(language, node) {
            return;
        }
        if is_c_family_pointer_expression(language, node) {
            return;
        }
        if is_call_node(language, node) {
            let Some(candidate_id) = analyses[analysis_index]
                .candidate_by_ast_and_kind
                .get(&(node.id(), MicroNodeKind::CallSite))
                .cloned()
            else {
                return;
            };
            let Ok(target_index) = resolve_direct_target(
                language,
                node,
                source,
                analysis_index,
                analyses,
                function_indices_by_name,
            ) else {
                return;
            };
            sources.push(ExpressionResultSource {
                candidate_id,
                exact_fact_ids: vec![analyses[target_index].scope.entity.id.clone()],
            });
            return;
        }
        if is_identifier_node(node) {
            let Some(candidate_id) = analyses[analysis_index]
                .candidate_by_ast_and_kind
                .get(&(node.id(), MicroNodeKind::ValueUse))
                .cloned()
            else {
                return;
            };
            let exact_fact_ids = node_text(node, source)
                .map(|name| normalize_binding_name(&name))
                .and_then(|name| analyses[analysis_index].bindings_by_name.get(&name))
                .and_then(|bindings| unique_binding(bindings))
                .map(|binding| vec![binding.entity_id.clone()])
                .unwrap_or_default();
            sources.push(ExpressionResultSource {
                candidate_id,
                exact_fact_ids,
            });
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            collect(
                language,
                source,
                child,
                analysis_index,
                analyses,
                function_indices_by_name,
                sources,
            );
        }
    }

    let mut raw_sources = Vec::new();
    collect(
        language,
        source,
        expression,
        analysis_index,
        analyses,
        function_indices_by_name,
        &mut raw_sources,
    );
    let mut merged = BTreeMap::<String, BTreeSet<String>>::new();
    for producer in raw_sources {
        merged
            .entry(producer.candidate_id)
            .or_default()
            .extend(producer.exact_fact_ids);
    }
    merged
        .into_iter()
        .map(|(candidate_id, exact_fact_ids)| ExpressionResultSource {
            candidate_id,
            exact_fact_ids: exact_fact_ids.into_iter().collect(),
        })
        .collect()
}

fn candidate_ids_within(
    analysis: &FunctionAnalysis<'_>,
    root: Node<'_>,
    kind: MicroNodeKind,
) -> Vec<String> {
    analysis
        .candidate_by_ast_and_kind
        .iter()
        .filter(|((node_id, candidate_kind), _)| {
            *candidate_kind == kind && node_contains(root, *node_id)
        })
        .map(|(_, candidate_id)| candidate_id.clone())
        .collect()
}

fn resolve_direct_target(
    language: SourceLanguage,
    call: Node<'_>,
    source: &str,
    caller_index: usize,
    analyses: &[FunctionAnalysis<'_>],
    function_indices_by_name: &BTreeMap<String, Vec<usize>>,
) -> Result<usize, String> {
    let raw_name = match language {
        SourceLanguage::Ruby => {
            if ruby_file_has_resolution_hazard(analyses[caller_index].scope.ast_node, source) {
                return Err(
                    "file-level Ruby open-class or metaprogramming syntax makes local-call resolution non-claimable"
                        .to_string(),
                );
            }
            if ruby_php_function_scope_gap(language, analyses[caller_index].scope.ast_node)
                .is_some()
            {
                return Err("Ruby caller is outside the safe top-level method domain".to_string());
            }
            let Some(name) = ruby_plain_unqualified_call_name(call, source) else {
                return Err(
                    "Ruby call is not one plain receiverless blockless identifier call".to_string(),
                );
            };
            if ruby_resolution_hazard(call, source) {
                return Err(format!(
                    "Ruby metaprogramming callee `{name}` is outside exact local-call proof"
                ));
            }
            name
        }
        SourceLanguage::Php => {
            if php_file_has_resolution_hazard(analyses[caller_index].scope.ast_node, source) {
                return Err(
                    "PHP include, require, eval, or imported-function syntax makes local-call resolution non-claimable"
                        .to_string(),
                );
            }
            if ruby_php_function_scope_gap(language, analyses[caller_index].scope.ast_node)
                .is_some()
            {
                return Err(
                    "PHP caller is outside the safe top-level named-function domain".to_string(),
                );
            }
            let Some(name) = php_plain_named_call_name(call, source) else {
                return Err(
                    "PHP call is not one plain named same-namespace function call".to_string(),
                );
            };
            name
        }
        _ => {
            let Some(name) = direct_call_callee_name(language, call, source) else {
                return Err(
                    "member, qualified, computed, or dynamic call is not an exact local call"
                        .to_string(),
                );
            };
            name
        }
    };
    let name = normalize_binding_name(&raw_name);
    if is_c_family_language(language) {
        if !c_family_call_has_simple_identifier_callee(call, source, &name) {
            return Err(format!(
                "C/C++ callee `{name}` is not a safe unqualified identifier call"
            ));
        }
        if looks_like_c_family_macro_name(&name) {
            return Err(format!(
                "C/C++ callee `{name}` is macro-style syntax, not an exact local call"
            ));
        }
        if !c_family_scope_is_safe_free_function(&analyses[caller_index].scope) {
            return Err(format!(
                "C/C++ caller `{}` is outside the safe top-level free-function domain",
                analyses[caller_index].scope.entity.name
            ));
        }
    }
    if language != SourceLanguage::Php
        && analyses[caller_index].bindings_by_name.contains_key(&name)
    {
        return Err(format!(
            "direct callee `{name}` is shadowed by a local binding"
        ));
    }
    let resolution_key =
        function_resolution_key(language, &analyses[caller_index].scope, &name, source);
    let Some(indices) = function_indices_by_name.get(&resolution_key) else {
        return Err(format!(
            "direct callee `{name}` has no exact same-file function in the caller domain"
        ));
    };
    if indices.len() != 1 {
        return Err(format!(
            "direct callee `{name}` has {} ambiguous same-file functions",
            indices.len()
        ));
    }
    let target_index = indices[0];
    if is_c_family_language(language)
        && !c_family_scope_is_safe_free_function(&analyses[target_index].scope)
    {
        return Err(format!(
            "C/C++ callee `{name}` is outside the safe top-level free-function definition domain"
        ));
    }
    if language == SourceLanguage::Ruby
        && ruby_php_function_scope_gap(language, analyses[target_index].scope.ast_node).is_some()
    {
        return Err(format!(
            "Ruby callee `{name}` is outside the safe top-level method domain"
        ));
    }
    if language == SourceLanguage::Php
        && ruby_php_function_scope_gap(language, analyses[target_index].scope.ast_node).is_some()
    {
        return Err(format!(
            "PHP callee `{name}` is outside the safe top-level named-function domain"
        ));
    }
    Ok(target_index)
}

fn resolve_direct_marked_target(
    language: SourceLanguage,
    call: Node<'_>,
    source: &str,
    caller: &FunctionAnalysis<'_>,
    analyses: &[FunctionAnalysis<'_>],
    function_indices_by_name: &BTreeMap<String, Vec<usize>>,
    marker_by_function_id: &BTreeMap<String, MarkerKind>,
) -> Option<(usize, MarkerKind)> {
    let caller_index = analyses
        .iter()
        .position(|analysis| analysis.scope.entity.id == caller.scope.entity.id)?;
    let index = resolve_direct_target(
        language,
        call,
        source,
        caller_index,
        analyses,
        function_indices_by_name,
    )
    .ok()?;
    let marker = *marker_by_function_id.get(&analyses[index].scope.entity.id)?;
    Some((index, marker))
}

fn is_call_node(language: SourceLanguage, node: Node<'_>) -> bool {
    match language {
        SourceLanguage::JavaScript
        | SourceLanguage::Jsx
        | SourceLanguage::TypeScript
        | SourceLanguage::Tsx
        | SourceLanguage::Go
        | SourceLanguage::Rust
        | SourceLanguage::C
        | SourceLanguage::Cpp => node.kind() == "call_expression",
        SourceLanguage::Python => node.kind() == "call",
        SourceLanguage::Java => node.kind() == "method_invocation",
        SourceLanguage::CSharp => node.kind() == "invocation_expression",
        SourceLanguage::Ruby => matches!(node.kind(), "call" | "command"),
        SourceLanguage::Php => node.kind() == "function_call_expression",
    }
}

fn is_native_assertion_node(language: SourceLanguage, node: Node<'_>) -> bool {
    matches!(language, SourceLanguage::Python | SourceLanguage::Java)
        && node.kind() == "assert_statement"
}

fn is_property_access_node(language: SourceLanguage, node: Node<'_>) -> bool {
    match language {
        SourceLanguage::JavaScript
        | SourceLanguage::Jsx
        | SourceLanguage::TypeScript
        | SourceLanguage::Tsx => {
            matches!(node.kind(), "member_expression" | "subscript_expression")
        }
        SourceLanguage::Python => matches!(node.kind(), "attribute" | "subscript"),
        SourceLanguage::Go => matches!(node.kind(), "selector_expression" | "index_expression"),
        SourceLanguage::Rust => matches!(node.kind(), "field_expression" | "index_expression"),
        SourceLanguage::Java => matches!(node.kind(), "field_access" | "array_access"),
        SourceLanguage::CSharp => matches!(
            node.kind(),
            "member_access_expression" | "element_access_expression"
        ),
        SourceLanguage::C | SourceLanguage::Cpp => {
            matches!(node.kind(), "field_expression" | "subscript_expression")
        }
        SourceLanguage::Ruby => matches!(node.kind(), "element_reference"),
        SourceLanguage::Php => matches!(
            node.kind(),
            "member_access_expression"
                | "nullsafe_member_access_expression"
                | "scoped_property_access_expression"
                | "subscript_expression"
        ),
    }
}

fn is_c_family_language(language: SourceLanguage) -> bool {
    matches!(language, SourceLanguage::C | SourceLanguage::Cpp)
}

fn ruby_php_function_scope_gap(
    language: SourceLanguage,
    function: Node<'_>,
) -> Option<(&'static str, &'static str)> {
    match language {
        SourceLanguage::Ruby => {
            let is_safe_top_level_method = function.kind() == "method"
                && function
                    .parent()
                    .is_some_and(|parent| parent.kind() == "program");
            (!is_safe_top_level_method).then_some((
                "unsupported_ruby_function_domain",
                "Ruby singleton, nested, class, module, and open-class methods are outside exact local function-flow proof",
            ))
        }
        SourceLanguage::Php => {
            if php_function_has_direct_reference_return(function) {
                return Some((
                    "unsupported_php_reference_return_function",
                    "PHP functions returning by reference cannot be exact local function-flow frames or call targets",
                ));
            }
            if php_function_has_owned_yield(function) {
                return Some((
                    "unsupported_php_generator_function",
                    "PHP generator functions cannot be ordinary local function-flow frames or call targets",
                ));
            }
            let parent = function.parent();
            let is_top_level = parent.is_some_and(|parent| parent.kind() == "program");
            let is_namespace_top_level = parent.is_some_and(|parent| {
                parent.kind() == "compound_statement"
                    && parent
                        .parent()
                        .is_some_and(|owner| owner.kind() == "namespace_definition")
            });
            (function.kind() != "function_definition"
                || (!is_top_level && !is_namespace_top_level))
                .then_some((
                    "unsupported_php_function_domain",
                    "PHP methods, closures, nested functions, and conditional functions are outside exact same-namespace local function-flow proof",
                ))
        }
        _ => None,
    }
}

fn language_owned_semantic_boundary(language: SourceLanguage, node: Node<'_>) -> bool {
    match language {
        SourceLanguage::Ruby => matches!(node.kind(), "block" | "do_block" | "lambda"),
        SourceLanguage::Php => matches!(node.kind(), "anonymous_function" | "arrow_function"),
        _ => false,
    }
}

fn php_function_has_direct_reference_return(function: Node<'_>) -> bool {
    let mut cursor = function.walk();
    let has_reference_return = function
        .named_children(&mut cursor)
        .any(|child| child.kind() == "reference_modifier");
    has_reference_return
}

fn php_function_has_owned_yield(function: Node<'_>) -> bool {
    fn contains_owned_yield(node: Node<'_>, owner_function_id: usize) -> bool {
        if node.id() != owner_function_id
            && (matches!(node.kind(), "anonymous_function" | "arrow_function")
                || generic_decl_kind(SourceLanguage::Php, node)
                    .is_some_and(is_function_entity_kind))
        {
            return false;
        }
        if node.kind() == "yield_expression" {
            return true;
        }
        let mut cursor = node.walk();
        let has_owned_yield = node
            .named_children(&mut cursor)
            .any(|child| contains_owned_yield(child, owner_function_id));
        has_owned_yield
    }

    function_body_node(function).is_some_and(|body| contains_owned_yield(body, function.id()))
}

fn language_parameter_is_unsupported(language: SourceLanguage, parameter: Node<'_>) -> bool {
    language == SourceLanguage::Php
        && (contains_named_kind(parameter, "reference_modifier")
            || contains_named_kind(parameter, "by_ref")
            || contains_named_kind(parameter, "property_promotion_parameter")
            || contains_named_kind(parameter, "constructor_promotion_parameter"))
}

fn language_binding_context_is_unsupported(
    language: SourceLanguage,
    node: Node<'_>,
    owner_function: Node<'_>,
) -> bool {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if candidate.id() == owner_function.id() {
            break;
        }
        let unsupported = match language {
            SourceLanguage::Ruby => matches!(
                candidate.kind(),
                "block"
                    | "do_block"
                    | "lambda"
                    | "instance_variable"
                    | "class_variable"
                    | "global_variable"
                    | "operator_assignment"
                    | "for"
                    | "rescue"
                    | "in"
                    | "in_clause"
                    | "rightward_assignment"
                    | "array_pattern"
                    | "find_pattern"
                    | "hash_pattern"
                    | "alternative_pattern"
                    | "as_pattern"
            ),
            SourceLanguage::Php => {
                matches!(
                    candidate.kind(),
                    "anonymous_function"
                        | "arrow_function"
                        | "dynamic_variable_name"
                        | "reference_assignment_expression"
                        | "by_ref"
                        | "reference_modifier"
                        | "global_declaration"
                        | "static_variable_declaration"
                        | "member_access_expression"
                        | "nullsafe_member_access_expression"
                        | "scoped_property_access_expression"
                        | "property_promotion_parameter"
                        | "constructor_promotion_parameter"
                ) || (candidate.kind().contains("parameter")
                    && language_parameter_is_unsupported(language, candidate))
            }
            _ => false,
        };
        if unsupported {
            return true;
        }
        current = candidate.parent();
    }
    false
}

fn language_assignment_target_is_unsupported(language: SourceLanguage, target: Node<'_>) -> bool {
    let parent_kind = target.parent().map(|parent| parent.kind());
    match language {
        SourceLanguage::Ruby => {
            matches!(
                target.kind(),
                "instance_variable" | "class_variable" | "global_variable"
            ) || matches!(parent_kind, Some("operator_assignment" | "for"))
                || contains_named_kind(target, "array_pattern")
                || contains_named_kind(target, "find_pattern")
                || contains_named_kind(target, "hash_pattern")
                || contains_named_kind(target, "rightward_assignment")
        }
        SourceLanguage::Php => {
            matches!(
                target.kind(),
                "dynamic_variable_name"
                    | "member_access_expression"
                    | "nullsafe_member_access_expression"
                    | "scoped_property_access_expression"
            ) || matches!(parent_kind, Some("reference_assignment_expression"))
        }
        _ => false,
    }
}

fn language_expression_result_is_unsupported(
    language: SourceLanguage,
    node: Node<'_>,
    source: &str,
) -> bool {
    match language {
        SourceLanguage::Ruby => {
            matches!(
                node.kind(),
                "block"
                    | "do_block"
                    | "lambda"
                    | "yield"
                    | "if_modifier"
                    | "unless_modifier"
                    | "if"
                    | "unless"
                    | "case"
                    | "case_match"
                    | "begin"
                    | "rescue"
                    | "ensure"
                    | "instance_variable"
                    | "class_variable"
                    | "global_variable"
                    | "operator_assignment"
                    | "for"
                    | "in"
                    | "in_clause"
                    | "rightward_assignment"
                    | "array_pattern"
                    | "find_pattern"
                    | "hash_pattern"
                    | "alternative_pattern"
                    | "as_pattern"
                    | "interpolation"
            ) || (matches!(node.kind(), "call" | "command")
                && (ruby_plain_unqualified_call_name(node, source).is_none()
                    || ruby_resolution_hazard(node, source)))
        }
        SourceLanguage::Php => {
            matches!(
                node.kind(),
                "member_call_expression"
                    | "nullsafe_member_call_expression"
                    | "scoped_call_expression"
                    | "object_creation_expression"
                    | "dynamic_variable_name"
                    | "include_expression"
                    | "include_once_expression"
                    | "require_expression"
                    | "require_once_expression"
                    | "anonymous_function"
                    | "arrow_function"
                    | "reference_assignment_expression"
                    | "by_ref"
                    | "reference_modifier"
                    | "global_declaration"
                    | "static_variable_declaration"
                    | "member_access_expression"
                    | "nullsafe_member_access_expression"
                    | "scoped_property_access_expression"
                    | "property_promotion_parameter"
                    | "constructor_promotion_parameter"
                    | "yield_expression"
            ) || (node.kind() == "function_call_expression"
                && node
                    .child_by_field_name("function")
                    .is_none_or(|callee| callee.kind() != "name"))
        }
        _ => false,
    }
}

fn language_identifier_context_is_unsupported(language: SourceLanguage, node: Node<'_>) -> bool {
    match language {
        SourceLanguage::Ruby => matches!(
            node.kind(),
            "block"
                | "do_block"
                | "lambda"
                | "yield"
                | "instance_variable"
                | "class_variable"
                | "global_variable"
                | "operator_assignment"
                | "for"
                | "in"
                | "in_clause"
                | "rightward_assignment"
                | "array_pattern"
                | "find_pattern"
                | "hash_pattern"
                | "alternative_pattern"
                | "as_pattern"
        ),
        SourceLanguage::Php => matches!(
            node.kind(),
            "anonymous_function"
                | "arrow_function"
                | "dynamic_variable_name"
                | "reference_assignment_expression"
                | "by_ref"
                | "reference_modifier"
                | "global_declaration"
                | "static_variable_declaration"
                | "property_promotion_parameter"
                | "constructor_promotion_parameter"
                | "yield_expression"
        ),
        _ => false,
    }
}

fn language_semantic_gap(
    language: SourceLanguage,
    node: Node<'_>,
    source: &str,
) -> Option<(&'static str, &'static str)> {
    match language {
        SourceLanguage::Ruby => match node.kind() {
            "block" | "do_block" | "lambda" => Some((
                "unsupported_ruby_closure_boundary",
                "Ruby block and lambda ownership, parameters, yield, numbered parameters, and nonlocal return are outside exact local flow proof",
            )),
            "yield" => Some((
                "unsupported_ruby_nonlocal_control_flow",
                "Ruby yield transfers control outside the exact local function-flow domain",
            )),
            "alias" => Some((
                "unsupported_ruby_metaprogramming",
                "Ruby alias changes the method-resolution domain dynamically",
            )),
            "class" | "module" | "singleton_class" | "singleton_method" => Some((
                "unsupported_ruby_open_class_semantics",
                "Ruby class, module, singleton, and open-class semantics are outside exact local function-flow proof",
            )),
            "if_modifier" | "unless_modifier" => Some((
                "unsupported_ruby_modifier_flow",
                "Ruby statement modifiers require path-sensitive implicit-tail semantics",
            )),
            "if" | "unless" | "case" | "case_match" | "begin" | "rescue" | "ensure" => {
                Some((
                    "unsupported_ruby_complex_control_flow",
                    "Ruby conditional, case, begin, rescue, and ensure flow is not an exact implicit-return path",
                ))
            }
            "instance_variable" | "class_variable" | "global_variable" => Some((
                "unsupported_ruby_nonlocal_binding",
                "Ruby instance, class, and global variables are not ordinary function-local bindings",
            )),
            "operator_assignment" => Some((
                "unsupported_ruby_operator_assignment",
                "Ruby operator assignment can invoke dynamic operators and cannot prove ordinary local value flow",
            )),
            "for" => Some((
                "unsupported_ruby_for_binding",
                "Ruby for-loop bindings escape the loop body and require nonlocal ownership semantics",
            )),
            "in" | "in_clause" | "rightward_assignment" | "array_pattern" | "find_pattern"
            | "hash_pattern" | "alternative_pattern" | "as_pattern" => Some((
                "unsupported_ruby_pattern_binding",
                "Ruby pattern matching and pattern bindings are outside exact local binding proof",
            )),
            "call" | "command" if ruby_resolution_hazard(node, source) => Some((
                "unsupported_ruby_metaprogramming",
                "Ruby dynamic send or metaprogramming changes the method-resolution domain",
            )),
            _ => None,
        },
        SourceLanguage::Php => match node.kind() {
            "anonymous_function" | "arrow_function" => Some((
                "unsupported_php_closure_boundary",
                "PHP closure ownership, captures, and arrow-function binding semantics are outside exact local flow proof",
            )),
            "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression"
            | "object_creation_expression" => Some((
                "unsupported_php_call_domain",
                "PHP member, nullsafe, scoped, and object-construction calls are outside exact local-call proof",
            )),
            "dynamic_variable_name" => Some((
                "unsupported_php_dynamic_variable",
                "PHP variable variables and dynamic callees cannot resolve to exact local bindings or functions",
            )),
            "include_expression"
            | "include_once_expression"
            | "require_expression"
            | "require_once_expression" => Some((
                "unsupported_php_dynamic_include",
                "PHP include and require expressions change the executable function domain dynamically",
            )),
            "reference_assignment_expression" | "by_ref" | "reference_modifier" => Some((
                "unsupported_php_reference_semantics",
                "PHP references, by-reference parameters, returns, captures, and assignments are outside exact local value-flow proof",
            )),
            "global_declaration" | "static_variable_declaration" => Some((
                "unsupported_php_nonlocal_binding",
                "PHP global and static declarations are not ordinary function-local bindings",
            )),
            "member_access_expression"
            | "nullsafe_member_access_expression"
            | "scoped_property_access_expression" => Some((
                "unsupported_php_property_flow",
                "PHP member, nullsafe, and scoped property access cannot prove an ordinary local target",
            )),
            "property_promotion_parameter" | "constructor_promotion_parameter" => Some((
                "unsupported_php_property_promotion",
                "PHP constructor property promotion is outside ordinary local parameter proof",
            )),
            "yield_expression" => Some((
                "unsupported_php_generator_flow",
                "PHP yield and generator state are outside exact local return-flow proof",
            )),
            _ => None,
        },
        _ => None,
    }
}

fn collect_language_semantic_gap_nodes<'tree>(
    language: SourceLanguage,
    node: Node<'tree>,
    source: &str,
    gaps: &mut Vec<(Node<'tree>, &'static str, &'static str)>,
) {
    if let Some((gap_kind, reason)) = language_semantic_gap(language, node, source) {
        gaps.push((node, gap_kind, reason));
        if language_owned_semantic_boundary(language, node) {
            return;
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_language_semantic_gap_nodes(language, child, source, gaps);
    }
}

fn ruby_resolution_hazard(node: Node<'_>, source: &str) -> bool {
    if matches!(
        node.kind(),
        "class" | "module" | "singleton_class" | "singleton_method" | "alias" | "undef"
    ) {
        return true;
    }
    if node.kind() == "method" {
        let is_nested_or_conditional = node
            .parent()
            .is_none_or(|parent| parent.kind() != "program");
        return is_nested_or_conditional
            || node
                .child_by_field_name("name")
                .and_then(|name| node_text(name, source))
                .is_some_and(|name| normalize_binding_name(&name) == "method_missing");
    }
    if !matches!(node.kind(), "call" | "command") {
        return false;
    }
    node.child_by_field_name("method")
        .or_else(|| node.child_by_field_name("name"))
        .and_then(|name| node_text(name, source))
        .or_else(|| direct_call_callee_name(SourceLanguage::Ruby, node, source))
        .is_some_and(|name| {
            matches!(
                normalize_binding_name(&name).as_str(),
                "define_method"
                    | "alias_method"
                    | "method_missing"
                    | "send"
                    | "public_send"
                    | "eval"
                    | "prepend"
                    | "include"
                    | "extend"
                    | "remove_method"
                    | "undef_method"
                    | "require"
                    | "require_relative"
                    | "load"
                    | "autoload"
                    | "class_eval"
                    | "module_eval"
                    | "instance_eval"
                    | "class_exec"
                    | "module_exec"
                    | "instance_exec"
                    | "define_singleton_method"
                    | "refine"
                    | "using"
                    | "module_function"
            )
        })
}

fn ruby_plain_unqualified_call_name(node: Node<'_>, source: &str) -> Option<String> {
    if node.kind() != "call"
        || node.child_by_field_name("receiver").is_some()
        || node.child_by_field_name("block").is_some()
    {
        return None;
    }
    let method = node
        .child_by_field_name("method")
        .or_else(|| node.child_by_field_name("name"))?;
    if method.kind() != "identifier" {
        return None;
    }
    let trailing_call_syntax = source.get(method.end_byte()..node.end_byte())?;
    if !trailing_call_syntax.starts_with('(') {
        return None;
    }
    node_text(method, source).map(|name| normalize_binding_name(&name))
}

fn php_plain_named_call_name(node: Node<'_>, source: &str) -> Option<String> {
    if node.kind() != "function_call_expression" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    if function.kind() != "name" {
        return None;
    }
    node_text(function, source).map(|name| normalize_binding_name(&name))
}

fn collect_file_level_ruby_resolution_hazards<'tree>(
    node: Node<'tree>,
    source: &str,
    hazards: &mut Vec<Node<'tree>>,
) {
    if ruby_resolution_hazard(node, source) {
        hazards.push(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_file_level_ruby_resolution_hazards(child, source, hazards);
    }
}

fn ruby_file_has_resolution_hazard(node: Node<'_>, source: &str) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut hazards = Vec::new();
    collect_file_level_ruby_resolution_hazards(root, source, &mut hazards);
    !hazards.is_empty()
}

fn c_family_claimability_boundary(
    language: SourceLanguage,
    function: Node<'_>,
) -> Option<&'static str> {
    if !is_c_family_language(language) {
        return None;
    }
    let mut current = function.parent();
    while let Some(parent) = current {
        if parent.kind() == "template_declaration" && language == SourceLanguage::Cpp {
            return Some("template_declaration");
        }
        if parent.kind().starts_with("preproc_") && parent.kind() != "preproc_include" {
            return Some("preprocessor_region");
        }
        current = parent.parent();
    }
    None
}

fn c_family_declarator_is_pointer_or_reference(node: Node<'_>) -> bool {
    contains_named_kind(node, "pointer_declarator")
        || contains_named_kind(node, "reference_declarator")
        || contains_named_kind(node, "abstract_pointer_declarator")
        || contains_named_kind(node, "abstract_reference_declarator")
}

fn is_c_family_pointer_expression(language: SourceLanguage, node: Node<'_>) -> bool {
    is_c_family_language(language)
        && matches!(node.kind(), "pointer_expression" | "address_of_expression")
}

fn c_family_pointer_expression_boundaries<'tree>(
    language: SourceLanguage,
    root: Node<'tree>,
) -> Vec<Node<'tree>> {
    fn collect<'tree>(
        language: SourceLanguage,
        node: Node<'tree>,
        boundaries: &mut Vec<Node<'tree>>,
    ) {
        if is_c_family_pointer_expression(language, node) {
            boundaries.push(node);
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            collect(language, child, boundaries);
        }
    }
    let mut boundaries = Vec::new();
    collect(language, root, &mut boundaries);
    boundaries
}

fn c_family_indirect_lvalue(language: SourceLanguage, node: Node<'_>) -> bool {
    is_c_family_language(language)
        && (is_property_access_node(language, node)
            || is_c_family_pointer_expression(language, node)
            || contains_named_kind(node, "field_expression")
            || contains_named_kind(node, "subscript_expression")
            || contains_named_kind(node, "pointer_expression"))
}

fn c_family_non_value_flow_binding_names(
    language: SourceLanguage,
    function: Node<'_>,
    owned_nodes: &[Node<'_>],
    source: &str,
) -> BTreeSet<String> {
    if !is_c_family_language(language) {
        return BTreeSet::new();
    }
    let mut names = BTreeSet::new();
    if let Some(parameters) = find_parameter_container(function) {
        let mut cursor = parameters.walk();
        for parameter in parameters.named_children(&mut cursor) {
            if c_family_declarator_is_pointer_or_reference(parameter) {
                if let Some(name) = simple_binding_name(parameter, source) {
                    names.insert(name);
                }
            }
        }
    }
    for node in owned_nodes.iter().copied() {
        let Some(parts) = assignment_parts(node) else {
            continue;
        };
        if c_family_declarator_is_pointer_or_reference(parts.left) {
            if let Some(name) = simple_binding_name(parts.left, source) {
                names.insert(name);
            }
        }
    }
    names
}

fn language_non_value_flow_binding_names(
    language: SourceLanguage,
    function: Node<'_>,
    owned_nodes: &[Node<'_>],
    source: &str,
) -> BTreeSet<String> {
    match language {
        SourceLanguage::C | SourceLanguage::Cpp => {
            c_family_non_value_flow_binding_names(language, function, owned_nodes, source)
        }
        SourceLanguage::Php => php_non_value_flow_binding_names(function, owned_nodes, source),
        _ => BTreeSet::new(),
    }
}

fn php_non_value_flow_binding_names(
    function: Node<'_>,
    owned_nodes: &[Node<'_>],
    source: &str,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(parameters) = find_parameter_container(function) {
        let mut cursor = parameters.walk();
        for parameter in parameters.named_children(&mut cursor) {
            if language_parameter_is_unsupported(SourceLanguage::Php, parameter) {
                collect_php_variable_names(parameter, source, &mut names);
            }
        }
    }

    for node in owned_nodes.iter().copied() {
        match node.kind() {
            "global_declaration" | "reference_assignment_expression" | "by_ref" => {
                collect_php_variable_names(node, source, &mut names);
            }
            "static_variable_declaration" => {
                if let Some(name) = node.child_by_field_name("name") {
                    collect_php_variable_names(name, source, &mut names);
                }
            }
            "anonymous_function" => {
                collect_php_by_ref_capture_names(node, source, &mut names);
            }
            _ => {}
        }
    }
    names
}

fn collect_php_variable_names(node: Node<'_>, source: &str, names: &mut BTreeSet<String>) {
    if node.kind() == "variable_name" {
        if let Some(name) = node_text(node, source) {
            names.insert(normalize_binding_name(&name));
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_php_variable_names(child, source, names);
    }
}

fn collect_php_by_ref_capture_names(closure: Node<'_>, source: &str, names: &mut BTreeSet<String>) {
    fn collect_use_clause(node: Node<'_>, source: &str, names: &mut BTreeSet<String>) {
        if node.kind() == "by_ref" {
            collect_php_variable_names(node, source, names);
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            collect_use_clause(child, source, names);
        }
    }

    let mut cursor = closure.walk();
    for child in closure.named_children(&mut cursor) {
        if child.kind() == "anonymous_function_use_clause" {
            collect_use_clause(child, source, names);
        }
    }
}

fn php_resolution_hazard(node: Node<'_>, source: &str) -> bool {
    if matches!(
        node.kind(),
        "include_expression"
            | "include_once_expression"
            | "require_expression"
            | "require_once_expression"
            | "eval_expression"
    ) {
        return true;
    }
    if node.kind() == "function_call_expression"
        && php_plain_named_call_name(node, source)
            .is_some_and(|name| name.eq_ignore_ascii_case("eval"))
    {
        return true;
    }
    node.kind() == "namespace_use_declaration"
        && node_text(node, source).is_some_and(|text| {
            text.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                .any(|word| word.eq_ignore_ascii_case("function"))
        })
}

fn collect_php_resolution_hazards<'tree>(
    node: Node<'tree>,
    source: &str,
    hazards: &mut Vec<Node<'tree>>,
) {
    if php_resolution_hazard(node, source) {
        hazards.push(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_php_resolution_hazards(child, source, hazards);
    }
}

fn php_file_has_resolution_hazard(node: Node<'_>, source: &str) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut hazards = Vec::new();
    collect_php_resolution_hazards(root, source, &mut hazards);
    !hazards.is_empty()
}

fn c_family_call_has_simple_identifier_callee(
    call: Node<'_>,
    source: &str,
    expected_name: &str,
) -> bool {
    call.child_by_field_name("function")
        .or_else(|| call.child_by_field_name("name"))
        .is_some_and(|callee| {
            callee.kind() == "identifier"
                && node_text(callee, source)
                    .map(|name| normalize_binding_name(&name) == expected_name)
                    .unwrap_or(false)
        })
}

fn looks_like_c_family_macro_name(name: &str) -> bool {
    let cased = name
        .chars()
        .filter(|ch| ch.is_alphabetic())
        .collect::<Vec<_>>();
    !cased.is_empty() && cased.iter().all(|ch| ch.is_uppercase())
}

fn c_family_scope_is_safe_free_function(scope: &FunctionScope<'_>) -> bool {
    if scope.entity.kind != EntityKind::Function || scope.ast_node.kind() != "function_definition" {
        return false;
    }
    let mut current = scope.ast_node.parent();
    while let Some(parent) = current {
        if matches!(
            parent.kind(),
            "class_specifier"
                | "struct_specifier"
                | "namespace_definition"
                | "template_declaration"
        ) || parent.kind().starts_with("preproc_")
        {
            return false;
        }
        current = parent.parent();
    }
    true
}

fn is_return_node(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "return_statement" | "return_expression" | "return"
    )
}

fn return_expression(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("argument")
        .or_else(|| node.child_by_field_name("value"))
        .or_else(|| node.child_by_field_name("expression"))
        .or_else(|| {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).last()
        })
        .filter(|expression| expression.id() != node.id())
}

#[derive(Debug, Clone, Copy)]
struct AssignmentParts<'tree> {
    left: Node<'tree>,
    right: Option<Node<'tree>>,
}

fn exact_single_call_expression(
    language: SourceLanguage,
    expression: Node<'_>,
) -> Option<Node<'_>> {
    if is_call_node(language, expression) {
        return Some(expression);
    }
    let mut cursor = expression.walk();
    let mut children = expression.named_children(&mut cursor);
    let only_child = children.next()?;
    if children.next().is_none() && is_call_node(language, only_child) {
        Some(only_child)
    } else {
        None
    }
}

fn assignment_parts(node: Node<'_>) -> Option<AssignmentParts<'_>> {
    if !matches!(
        node.kind(),
        "variable_declarator"
            | "init_declarator"
            | "let_declaration"
            | "assignment"
            | "assignment_expression"
            | "assignment_statement"
            | "augmented_assignment"
            | "augmented_assignment_expression"
            | "compound_assignment_expr"
            | "operator_assignment"
            | "short_var_declaration"
            | "update_expression"
    ) {
        return None;
    }
    let left = node
        .child_by_field_name("left")
        .or_else(|| node.child_by_field_name("name"))
        .or_else(|| node.child_by_field_name("pattern"))
        .or_else(|| node.child_by_field_name("declarator"))
        .or_else(|| node.child_by_field_name("argument"))
        .or_else(|| {
            let mut cursor = node.walk();
            let first = node.named_children(&mut cursor).next();
            first
        })?;
    let right = node
        .child_by_field_name("right")
        .or_else(|| node.child_by_field_name("value"))
        .or_else(|| {
            let mut cursor = node.walk();
            let last = node.named_children(&mut cursor).last();
            last
        })
        .filter(|right| right.id() != left.id());
    Some(AssignmentParts { left, right })
}

fn is_declaration_assignment(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "variable_declarator" | "init_declarator" | "let_declaration" | "short_var_declaration"
    )
}

fn is_synthesizable_declaration_assignment(node: Node<'_>) -> bool {
    if !is_declaration_assignment(node) {
        return false;
    }
    if !matches!(node.kind(), "variable_declarator" | "init_declarator") {
        return true;
    }
    let Some(parent) = node.parent() else {
        return true;
    };
    let mut cursor = parent.walk();
    let sibling_declarator_count = parent
        .named_children(&mut cursor)
        .filter(|child| {
            matches!(
                child.kind(),
                "variable_declarator" | "init_declarator" | "let_declaration"
            )
        })
        .count();
    sibling_declarator_count <= 1
}

fn is_implicit_local_declaration_node(language: SourceLanguage, node: Node<'_>) -> bool {
    match language {
        SourceLanguage::Python | SourceLanguage::Ruby => node.kind() == "assignment",
        SourceLanguage::Php => node.kind() == "assignment_expression",
        _ => false,
    }
}

fn simple_binding_name(node: Node<'_>, source: &str) -> Option<String> {
    fn collect(node: Node<'_>, source: &str, names: &mut Vec<String>) {
        if is_identifier_node(node) {
            if let Some(name) = node_text(node, source) {
                names.push(normalize_binding_name(&name));
            }
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            collect(child, source, names);
        }
    }
    let mut names = Vec::new();
    collect(node, source, &mut names);
    names.sort();
    names.dedup();
    (names.len() == 1).then(|| names.remove(0))
}

fn is_condition_node(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "if_statement"
            | "if_expression"
            | "if"
            | "unless"
            | "while_statement"
            | "while_expression"
            | "match_expression"
            | "conditional_expression"
            | "ternary_expression"
    )
}

fn condition_expression(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("condition")
        .or_else(|| node.child_by_field_name("predicate"))
        .or_else(|| node.child_by_field_name("test"))
        .or_else(|| node.child_by_field_name("value"))
        .or_else(|| {
            let mut cursor = node.walk();
            let first = node.named_children(&mut cursor).next();
            first
        })
}

fn condition_branch_arms<'tree>(
    node: Node<'tree>,
    condition: Node<'tree>,
) -> Vec<(String, Node<'tree>)> {
    if node.kind() == "match_expression" {
        let Some(body) = node.child_by_field_name("body") else {
            return Vec::new();
        };
        let mut cursor = body.walk();
        return body
            .named_children(&mut cursor)
            .filter(|arm| arm.kind() == "match_arm")
            .enumerate()
            .map(|(index, arm)| (format!("match_{index}"), arm))
            .collect();
    }
    let mut arms: Vec<(String, Node<'tree>)> = Vec::new();
    for (field, label) in [
        ("consequence", "then"),
        ("body", "then"),
        ("alternative", "else"),
    ] {
        if let Some(arm) = node.child_by_field_name(field) {
            if arm.id() != condition.id() && !arms.iter().any(|(_, seen)| seen.id() == arm.id()) {
                arms.push((label.to_string(), arm));
            }
        }
    }
    if arms.is_empty() {
        let mut cursor = node.walk();
        for (index, child) in node
            .named_children(&mut cursor)
            .filter(|child| child.id() != condition.id())
            .enumerate()
        {
            arms.push((if index == 0 { "then" } else { "else" }.to_string(), child));
        }
    }
    arms
}

fn is_identifier_node(node: Node<'_>) -> bool {
    matches!(node.kind(), "identifier" | "variable_name")
}

fn identifier_is_value_use(
    language: SourceLanguage,
    node: Node<'_>,
    owner_function: Node<'_>,
) -> bool {
    let wanted_id = node.id();
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.id() == owner_function.id() {
            break;
        }
        if language_identifier_context_is_unsupported(language, parent) {
            return false;
        }
        if language == SourceLanguage::Php
            && matches!(
                parent.kind(),
                "member_call_expression"
                    | "nullsafe_member_call_expression"
                    | "scoped_call_expression"
            )
            && parent
                .child_by_field_name("name")
                .is_some_and(|name| node_contains(name, wanted_id))
        {
            return false;
        }
        if let Some(parts) = assignment_parts(parent) {
            if node_contains(parts.left, wanted_id) {
                return false;
            }
        }
        if is_c_family_pointer_expression(language, parent)
            || (is_c_family_language(language)
                && matches!(
                    parent.kind(),
                    "pointer_declarator"
                        | "reference_declarator"
                        | "abstract_pointer_declarator"
                        | "abstract_reference_declarator"
                ))
        {
            return false;
        }
        if is_call_node(language, parent) {
            for field in ["function", "name", "method"] {
                if parent
                    .child_by_field_name(field)
                    .is_some_and(|callee| node_contains(callee, wanted_id))
                {
                    return false;
                }
            }
        }
        if is_property_access_node(language, parent) {
            for field in ["property", "field", "name", "member"] {
                if parent
                    .child_by_field_name(field)
                    .is_some_and(|property| node_contains(property, wanted_id))
                {
                    return false;
                }
            }
        }
        if matches!(
            parent.kind(),
            "required_parameter"
                | "optional_parameter"
                | "parameter"
                | "formal_parameter"
                | "type_identifier"
                | "primitive_type"
        ) {
            return false;
        }
        current = parent.parent();
    }
    true
}

fn normalize_binding_name(name: &str) -> String {
    name.trim().trim_start_matches(['$', '@']).to_string()
}

fn compact_node_label(node: Node<'_>, source: &str) -> String {
    node_text(node, source)
        .map(|text| bounded_label(&text.split_whitespace().collect::<Vec<_>>().join(" ")))
        .unwrap_or_else(|| node.kind().to_string())
}

fn bounded_label(label: &str) -> String {
    label.chars().take(160).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{extract_parser_fact_bundle, TreeSitterParser};

    fn node_matrix_source(language: SourceLanguage) -> &'static str {
        match language {
            SourceLanguage::JavaScript | SourceLanguage::Jsx => {
                r#"
// @codegraph-sanitizer
function sanitizeValue(value) { return value; }
// @codegraph-assertion
function assertValue(value) { return value; }
function run(input) {
  let clean = sanitizeValue(input);
  let count = 0;
  count += 1;
  if (clean) { assertValue(clean); } else { clean = input; }
  const record = { value: clean };
  const probe = record.value;
  return clean;
}
"#
            }
            SourceLanguage::TypeScript | SourceLanguage::Tsx => {
                r#"
// @codegraph-sanitizer
function sanitizeValue(value: number): number { return value; }
// @codegraph-assertion
function assertValue(value: number): number { return value; }
function run(input: number): number {
  let clean = sanitizeValue(input);
  let count = 0;
  count += 1;
  if (clean) { assertValue(clean); } else { clean = input; }
  const record = { value: clean };
  const probe = record.value;
  return clean;
}
"#
            }
            SourceLanguage::Python => {
                r#"
# @codegraph-sanitizer
def sanitize_value(value):
    return value
# @codegraph-assertion
def assert_value(value):
    return value
def run(input):
    clean = sanitize_value(input)
    count = 0
    count += 1
    if clean:
        assert_value(clean)
    else:
        clean = input
    record = {"value": clean}
    probe = record["value"]
    return clean
"#
            }
            SourceLanguage::Go => {
                r#"
package sample
// @codegraph-sanitizer
func sanitizeValue(value int) int { return value }
// @codegraph-assertion
func assertValue(value int) int { return value }
func run(input int) int {
    clean := sanitizeValue(input)
    count := 0
    count += 1
    if clean > 0 { assertValue(clean) } else { clean = input }
    record := struct { value int }{value: clean}
    probe := record.value
    _ = probe
    return clean
}
"#
            }
            SourceLanguage::Rust => {
                r#"
// @codegraph-sanitizer
fn sanitize_value(value: i32) -> i32 { value }
// @codegraph-assertion
fn assert_value(value: i32) -> i32 { value }
fn run(input: i32) -> i32 {
    let mut clean = sanitize_value(input);
    let mut count = 0;
    count += 1;
    if clean > 0 { assert_value(clean); } else { clean = input; }
    let record = (clean,);
    let probe = record.0;
    return clean;
}
"#
            }
            SourceLanguage::Java => {
                r#"
class Sample {
  // @codegraph-sanitizer
  static int sanitizeValue(int value) { return value; }
  // @codegraph-assertion
  static int assertValue(int value) { return value; }
  static int run(int input) {
    int clean = sanitizeValue(input);
    int count = 0;
    count += 1;
    if (clean > 0) { assertValue(clean); } else { clean = input; }
    int[] record = new int[] { clean };
    int probe = record.length;
    return clean;
  }
}
"#
            }
            SourceLanguage::CSharp => {
                r#"
class Sample {
  // @codegraph-sanitizer
  static int SanitizeValue(int value) { return value; }
  // @codegraph-assertion
  static int AssertValue(int value) { return value; }
  static int Run(int input) {
    int clean = SanitizeValue(input);
    int count = 0;
    count += 1;
    if (clean > 0) { AssertValue(clean); } else { clean = input; }
    int[] record = new int[] { clean };
    int probe = record.Length;
    return clean;
  }
}
"#
            }
            SourceLanguage::C => {
                r#"
struct Record { int value; };
// @codegraph-sanitizer
int sanitize_value(int value) { return value; }
// @codegraph-assertion
int assert_value(int value) { return value; }
int run(int input) {
  int clean = sanitize_value(input);
  int count = 0;
  count += 1;
  if (clean > 0) { assert_value(clean); } else { clean = input; }
  struct Record record = { clean };
  int probe = record.value;
  return clean;
}
"#
            }
            SourceLanguage::Cpp => {
                r#"
struct Record { int value; };
// @codegraph-sanitizer
int sanitize_value(int value) { return value; }
// @codegraph-assertion
int assert_value(int value) { return value; }
int run(int input) {
  int clean = sanitize_value(input);
  int count = 0;
  count += 1;
  if (clean > 0) { assert_value(clean); } else { clean = input; }
  Record record { clean };
  int probe = record.value;
  return clean;
}
"#
            }
            SourceLanguage::Ruby => {
                r#"
# @codegraph-sanitizer
def sanitize_value(value)
  value
end
# @codegraph-assertion
def assert_value(value)
  value
end
def run(input)
  clean = sanitize_value(input)
  count = 0
  count += 1
  if clean
    assert_value(clean)
  else
    clean = input
  end
  record = { value: clean }
  probe = record[:value]
  return clean
end
"#
            }
            SourceLanguage::Php => {
                r#"
<?php
// @codegraph-sanitizer
function sanitize_value($value) { return $value; }
// @codegraph-assertion
function assert_value($value) { return $value; }
function run($input) {
  $clean = sanitize_value($input);
  $count = 0;
  $count += 1;
  if ($clean) { assert_value($clean); } else { $clean = $input; }
  $record = (object) ["value" => $clean];
  $probe = $record->value;
  return $clean;
}
"#
            }
        }
    }

    fn extension(language: SourceLanguage) -> &'static str {
        match language {
            SourceLanguage::JavaScript => "js",
            SourceLanguage::Jsx => "jsx",
            SourceLanguage::TypeScript => "ts",
            SourceLanguage::Tsx => "tsx",
            SourceLanguage::Python => "py",
            SourceLanguage::Go => "go",
            SourceLanguage::Rust => "rs",
            SourceLanguage::Java => "java",
            SourceLanguage::CSharp => "cs",
            SourceLanguage::C => "c",
            SourceLanguage::Cpp => "cpp",
            SourceLanguage::Ruby => "rb",
            SourceLanguage::Php => "php",
        }
    }

    fn extract(language: SourceLanguage, path_prefix: &str) -> ParserFactsV1Report {
        let path = format!("{path_prefix}/sample.{}", extension(language));
        let source = node_matrix_source(language);
        extract_source(language, &path, source)
    }

    fn extract_source(language: SourceLanguage, path: &str, source: &str) -> ParserFactsV1Report {
        let parsed = TreeSitterParser
            .parse_source(&path, source, language)
            .unwrap_or_else(|error| panic!("parse {path}: {error}"));
        let bundle = extract_parser_fact_bundle(&parsed, source);
        ParserFactsV1::extract(&parsed, source, &bundle)
    }

    fn extract_active_source(
        language: SourceLanguage,
        path: &str,
        source: &str,
    ) -> ParserFactsV1Report {
        let parsed = TreeSitterParser
            .parse_source(path, source, language)
            .unwrap_or_else(|error| panic!("parse {path}: {error}"));
        let bundle = extract_parser_fact_bundle(&parsed, source);
        ParserFactsV1::extract_active(&parsed, source, &bundle)
    }

    #[test]
    fn rust_implicit_identifier_tail_emits_a_claimable_complete_return_path() {
        let report = extract_active_source(
            SourceLanguage::Rust,
            "src/parser-facts-v1/rust-tail-id.rs",
            "fn id(x: i32) -> i32 { x }",
        );
        assert_eq!(report.status, "complete_active", "{report:#?}");
        assert!(report.gaps.is_empty(), "{report:#?}");
        let return_sites = report
            .nodes
            .iter()
            .filter(|node| node.node_kind == MicroNodeKind::ReturnSite)
            .collect::<Vec<_>>();
        assert_eq!(return_sites.len(), 1, "{report:#?}");
        let return_site = return_sites[0];
        assert_eq!(return_site.source_span.start_line, 1);
        assert_eq!(return_site.name_or_literal, "return");
        assert_eq!(
            return_site.claimability,
            MVP4_3_PARSER_FACTS_V1_CLAIMABILITY
        );
        let returned_value = report
            .nodes
            .iter()
            .find(|node| {
                node.node_kind == MicroNodeKind::ValueUse
                    && node.name_or_literal == "x"
                    && node
                        .source_span
                        .start_column
                        .is_some_and(|column| column > 20)
            })
            .unwrap_or_else(|| panic!("missing tail value use: {report:#?}"));
        let flow = report
            .edges
            .iter()
            .find(|edge| {
                edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                    && edge.head_micro_node_id == returned_value.micro_node_id
                    && edge.tail_micro_node_id == return_site.micro_node_id
            })
            .unwrap_or_else(|| panic!("missing tail flow: {report:#?}"));
        assert_eq!(flow.exactness, MicroExactness::DerivedWithProvenance);
        assert!(flow
            .provenance
            .source_fact_ids
            .contains(&returned_value.micro_node_id));
        assert!(report.edges.iter().any(|edge| {
            edge.micro_edge_kind == MicroEdgeKind::LocalReturnsTo
                && edge.head_micro_node_id == return_site.micro_node_id
                && edge.exactness == MicroExactness::Exact
        }));
        assert!(report.nodes.iter().all(|node| {
            node.source_span.repo_relative_path == "src/parser-facts-v1/rust-tail-id.rs"
                && node.source_span.start_line <= node.source_span.end_line
                && node.source_span.start_column.is_some()
                && node.source_span.end_column.is_some()
        }));
        assert!(report.edges.iter().all(|edge| {
            edge.relation_source_span.repo_relative_path == "src/parser-facts-v1/rust-tail-id.rs"
                && edge.relation_source_span.start_line <= edge.relation_source_span.end_line
                && edge.relation_source_span.start_column.is_some()
                && edge.relation_source_span.end_column.is_some()
                && edge.claimability == MVP4_3_PARSER_FACTS_V1_CLAIMABILITY
        }));
    }

    #[test]
    fn rust_explicit_early_return_and_implicit_normal_tail_stay_distinct() {
        let report = extract_source(
            SourceLanguage::Rust,
            "src/parser-facts-v1/rust-early-and-tail.rs",
            "fn choose(x: i32) -> i32 {\n    if x == 0 { return x; }\n    x\n}\n",
        );
        let returns = report
            .nodes
            .iter()
            .filter(|node| node.node_kind == MicroNodeKind::ReturnSite)
            .collect::<Vec<_>>();
        assert_eq!(returns.len(), 2, "{report:#?}");
        assert_eq!(
            returns
                .iter()
                .map(|node| node.source_span.start_line)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([2, 3])
        );
        for return_site in returns {
            assert!(report.edges.iter().any(|edge| {
                edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                    && edge.tail_micro_node_id == return_site.micro_node_id
            }));
            assert!(report.edges.iter().any(|edge| {
                edge.micro_edge_kind == MicroEdgeKind::LocalReturnsTo
                    && edge.head_micro_node_id == return_site.micro_node_id
            }));
        }
    }

    #[test]
    fn rust_block_if_and_match_tails_emit_path_local_return_sites() {
        let report = extract_source(
            SourceLanguage::Rust,
            "src/parser-facts-v1/rust-compound-tails.rs",
            "fn block_tail(x: i32) -> i32 { { x } }\n\
             fn if_tail(x: i32) -> i32 { if x > 0 { x } else { x } }\n\
             fn match_tail(x: i32) -> i32 { match x { 0 => x, _ => { x } } }\n",
        );
        let returns = report
            .nodes
            .iter()
            .filter(|node| node.node_kind == MicroNodeKind::ReturnSite)
            .collect::<Vec<_>>();
        assert_eq!(returns.len(), 5, "{report:#?}");
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalReturnsTo)
                .count(),
            5,
            "{report:#?}"
        );
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                        && returns
                            .iter()
                            .any(|site| site.micro_node_id == edge.tail_micro_node_id)
                })
                .count(),
            5,
            "{report:#?}"
        );
        let guarded_returns = report
            .edges
            .iter()
            .filter(|edge| {
                edge.micro_edge_kind == MicroEdgeKind::LocalGuards
                    && returns
                        .iter()
                        .any(|site| site.micro_node_id == edge.tail_micro_node_id)
            })
            .collect::<Vec<_>>();
        assert_eq!(guarded_returns.len(), 4, "{report:#?}");
        assert_eq!(
            guarded_returns
                .iter()
                .map(|edge| edge.head_micro_node_id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            4,
            "each if/match value path must retain its own branch-arm identity"
        );
    }

    #[test]
    fn rust_unit_semicolon_declaration_and_recovery_tails_never_fabricate_returns() {
        let report = extract_source(
            SourceLanguage::Rust,
            "src/parser-facts-v1/rust-tail-negatives.rs",
            "fn implicit_unit(x: i32) { x }\n\
             fn explicit_unit(x: i32) -> () { x }\n\
             fn semicolon(x: i32) -> i32 { x; }\n\
             fn declaration(x: i32) -> i32 { let y = x; }\n\
             fn explicit(x: i32) -> i32 { return x; }\n",
        );
        let returns = report
            .nodes
            .iter()
            .filter(|node| node.node_kind == MicroNodeKind::ReturnSite)
            .collect::<Vec<_>>();
        assert_eq!(returns.len(), 1, "{report:#?}");
        assert_eq!(returns[0].source_span.start_line, 5, "{report:#?}");
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo)
                .filter(|edge| edge.tail_micro_node_id == returns[0].micro_node_id)
                .count(),
            1,
            "explicit return flow must remain unchanged: {report:#?}"
        );
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalReturnsTo)
                .count(),
            1,
            "explicit return ownership must remain unchanged: {report:#?}"
        );

        let recovery = extract_source(
            SourceLanguage::Rust,
            "src/parser-facts-v1/rust-invalid-tail.rs",
            "fn broken(x: i32) -> i32 { if x > 0 { x } else {",
        );
        assert_eq!(recovery.status, "parse_recovery_excluded", "{recovery:#?}");
        assert!(recovery.nodes.is_empty(), "{recovery:#?}");
        assert!(recovery.edges.is_empty(), "{recovery:#?}");
    }

    #[test]
    fn active_parser_facts_extensions_cover_the_full_semantic_registry() {
        let expected_nodes = codegraph_core::MVP4_3_LOCAL_MICRO_FLOW_PACKET_NODE_KINDS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let expected_edges = MicroEdgeKind::ALL.iter().copied().collect::<BTreeSet<_>>();
        for (language, extension) in [
            (SourceLanguage::JavaScript, "js"),
            (SourceLanguage::JavaScript, "mjs"),
            (SourceLanguage::JavaScript, "cjs"),
            (SourceLanguage::Jsx, "jsx"),
            (SourceLanguage::TypeScript, "mts"),
            (SourceLanguage::TypeScript, "cts"),
            (SourceLanguage::Tsx, "tsx"),
            (SourceLanguage::Python, "py"),
            (SourceLanguage::Go, "go"),
            (SourceLanguage::Rust, "rs"),
            (SourceLanguage::Java, "java"),
            (SourceLanguage::CSharp, "cs"),
            (SourceLanguage::C, "c"),
            (SourceLanguage::C, "h"),
            (SourceLanguage::Cpp, "cc"),
            (SourceLanguage::Cpp, "cpp"),
            (SourceLanguage::Cpp, "cxx"),
            (SourceLanguage::Cpp, "hpp"),
            (SourceLanguage::Cpp, "hh"),
            (SourceLanguage::Cpp, "hxx"),
            (SourceLanguage::Ruby, "rb"),
            (SourceLanguage::Php, "php"),
        ] {
            let path = format!("src/parser-facts-v1/active/sample.{extension}");
            let report = extract_active_source(language, &path, node_matrix_source(language));
            assert_eq!(report.status, "complete_active", "{path}: {report:#?}");
            assert!(report.production_activation_enabled, "{path}");
            let expected_frontend =
                codegraph_core::mvp4_language_frontend_contract(language.as_str())
                    .expect("canonical frontend")
                    .frontend;
            assert_eq!(report.frontend, expected_frontend, "{path}");
            assert!(
                report
                    .nodes
                    .iter()
                    .all(|node| node.frontend == expected_frontend),
                "{path}"
            );
            assert!(
                report
                    .edges
                    .iter()
                    .all(|edge| edge.frontend == expected_frontend),
                "{path}"
            );
            assert_eq!(
                report
                    .nodes
                    .iter()
                    .map(|node| node.node_kind)
                    .collect::<BTreeSet<_>>(),
                expected_nodes,
                "{path}"
            );
            assert_eq!(
                report
                    .edges
                    .iter()
                    .map(|edge| edge.micro_edge_kind)
                    .collect::<BTreeSet<_>>(),
                expected_edges,
                "{path}"
            );
            assert!(
                report.nodes.iter().all(|node| {
                    node.claimability == MVP4_3_PARSER_FACTS_V1_CLAIMABILITY
                        && node.extraction_version
                            == MVP4_3_PARSER_FACTS_V1_MICRO_FACT_EXTRACTION_VERSION
                        && node.provenance.extractor_or_adapter_version
                            == MVP4_3_PARSER_FACTS_V1_MICRO_FACT_EXTRACTION_VERSION
                        && node.provenance.limitations.iter().any(|limitation| {
                            limitation == "same_file_intraprocedural_static_semantics_only"
                        })
                        && !node.provenance.limitations.iter().any(|limitation| {
                            limitation == "parser_facts_v1_is_not_production_activated"
                        })
                }),
                "{path}"
            );
            assert!(
                report.edges.iter().all(|edge| {
                    edge.claimability == MVP4_3_PARSER_FACTS_V1_CLAIMABILITY
                        && edge.extraction_version
                            == MVP4_3_PARSER_FACTS_V1_MICRO_FACT_EXTRACTION_VERSION
                        && edge.provenance.extractor_or_adapter_version
                            == MVP4_3_PARSER_FACTS_V1_MICRO_FACT_EXTRACTION_VERSION
                        && edge.provenance.limitations.iter().any(|limitation| {
                            limitation == "same_file_intraprocedural_static_semantics_only"
                        })
                        && !edge.provenance.limitations.iter().any(|limitation| {
                            limitation == "parser_facts_v1_is_not_production_activated"
                        })
                }),
                "{path}"
            );
            for edge in &report.edges {
                let derived = matches!(
                    edge.micro_edge_kind,
                    MicroEdgeKind::LocalFlowsTo
                        | MicroEdgeKind::LocalMutates
                        | MicroEdgeKind::LocalSanitizes
                        | MicroEdgeKind::LocalGuards
                        | MicroEdgeKind::LocalAsserts
                );
                assert_eq!(
                    edge.exactness,
                    if derived {
                        MicroExactness::DerivedWithProvenance
                    } else {
                        MicroExactness::Exact
                    },
                    "{path}/{:?}",
                    edge.micro_edge_kind
                );
                if derived {
                    assert!(!edge.provenance.source_fact_ids.is_empty(), "{path}");
                    assert!(!edge.provenance.source_spans.is_empty(), "{path}");
                    codegraph_core::validate_micro_fact_provenance(&edge.provenance)
                        .unwrap_or_else(|error| panic!("{path}: {error}: {edge:#?}"));
                }
            }
        }
    }

    #[test]
    fn node_inventory_and_function_ownership_cover_all_canonical_frontends() {
        let required = BTreeSet::from([
            MicroNodeKind::FunctionFrame,
            MicroNodeKind::Parameter,
            MicroNodeKind::LocalBinding,
            MicroNodeKind::PropertyAccess,
            MicroNodeKind::CallSite,
            MicroNodeKind::ReturnSite,
            MicroNodeKind::AssignmentSite,
            MicroNodeKind::ValueUse,
            MicroNodeKind::MutationSite,
            MicroNodeKind::ConditionSite,
            MicroNodeKind::BranchArm,
            MicroNodeKind::TestAssertion,
            MicroNodeKind::SanitizerCall,
        ]);
        for language in SourceLanguage::ALL {
            let report = extract(*language, "src/parser-facts-v1");
            assert_eq!(
                report.status, "complete_inactive",
                "{language:?}: {report:#?}"
            );
            assert!(!report.production_activation_enabled, "{language:?}");
            let kinds = report
                .nodes
                .iter()
                .map(|node| node.node_kind)
                .collect::<BTreeSet<_>>();
            assert!(
                required.is_subset(&kinds),
                "{language:?} missing {:?}: {report:#?}",
                required.difference(&kinds).collect::<Vec<_>>()
            );
            let function_ids = report
                .nodes
                .iter()
                .filter(|node| node.node_kind == MicroNodeKind::FunctionFrame)
                .map(|node| node.enclosing_function_identity.as_str())
                .collect::<BTreeSet<_>>();
            assert!(report.nodes.iter().all(|node| {
                node.source_span.start_line > 0
                    && function_ids.contains(node.enclosing_function_identity.as_str())
                    && node.claimability == PARSER_FACTS_V1_CLAIMABILITY
            }));
        }
    }

    #[test]
    fn exact_relation_gate_covers_all_frontends_with_canonical_endpoints() {
        let required = BTreeSet::from([
            MicroEdgeKind::LocalReads,
            MicroEdgeKind::LocalWrites,
            MicroEdgeKind::LocalCalls,
            MicroEdgeKind::LocalReturnsTo,
            MicroEdgeKind::LocalChecks,
            MicroEdgeKind::LocalBranchesTo,
        ]);
        for language in SourceLanguage::ALL {
            let report = extract(*language, "src/parser-facts-v1");
            let kinds = report
                .edges
                .iter()
                .map(|edge| edge.micro_edge_kind)
                .collect::<BTreeSet<_>>();
            assert!(
                required.is_subset(&kinds),
                "{language:?} missing exact relations {:?}: {report:#?}",
                required.difference(&kinds).collect::<Vec<_>>()
            );
            let nodes = report
                .nodes
                .iter()
                .map(|node| (node.micro_node_id.as_str(), node))
                .collect::<BTreeMap<_, _>>();
            for edge in report
                .edges
                .iter()
                .filter(|edge| required.contains(&edge.micro_edge_kind))
            {
                assert_eq!(
                    edge.exactness,
                    MicroExactness::Exact,
                    "{language:?}: {edge:#?}"
                );
                assert_eq!(edge.claimability, PARSER_FACTS_V1_CLAIMABILITY);
                assert!(edge.relation_source_span.start_line > 0);
                let head = nodes[edge.head_micro_node_id.as_str()];
                let tail = nodes[edge.tail_micro_node_id.as_str()];
                assert!(
                    codegraph_core::mvp4_micro_edge_endpoint_pairs(edge.micro_edge_kind)
                        .iter()
                        .any(|pair| pair.head == head.node_kind && pair.tail == tail.node_kind)
                );
                assert!(
                    codegraph_core::mvp4_micro_edge_allowed_derivations(edge.micro_edge_kind)
                        .contains(&edge.provenance.derivation_kind)
                );
                codegraph_core::validate_micro_fact_provenance(&edge.provenance)
                    .unwrap_or_else(|error| panic!("{language:?}: {error}: {edge:#?}"));
            }
        }
    }

    #[test]
    fn derived_relation_gate_covers_all_frontends_with_canonical_provenance() {
        let required = BTreeSet::from([
            MicroEdgeKind::LocalFlowsTo,
            MicroEdgeKind::LocalMutates,
            MicroEdgeKind::LocalGuards,
        ]);
        for language in SourceLanguage::ALL {
            let report = extract(*language, "src/parser-facts-v1");
            let kinds = report
                .edges
                .iter()
                .map(|edge| edge.micro_edge_kind)
                .collect::<BTreeSet<_>>();
            assert!(
                required.is_subset(&kinds),
                "{language:?} missing derived relations {:?}: {report:#?}",
                required.difference(&kinds).collect::<Vec<_>>()
            );
            let nodes = report
                .nodes
                .iter()
                .map(|node| (node.micro_node_id.as_str(), node))
                .collect::<BTreeMap<_, _>>();
            let mut mutated_names = BTreeSet::new();
            for edge in report
                .edges
                .iter()
                .filter(|edge| required.contains(&edge.micro_edge_kind))
            {
                assert_eq!(
                    edge.exactness,
                    MicroExactness::DerivedWithProvenance,
                    "{language:?}: {edge:#?}"
                );
                assert!(!edge.provenance.source_fact_ids.is_empty());
                assert!(edge
                    .provenance
                    .source_fact_ids
                    .iter()
                    .any(|fact_id| fact_id.starts_with("parser_facts_v1_role:")));
                assert!(edge
                    .provenance
                    .source_fact_ids
                    .contains(&edge.head_micro_node_id));
                assert!(edge
                    .provenance
                    .source_spans
                    .contains(&edge.relation_source_span));
                assert!(edge.provenance.source_spans.len() >= 2);
                let head = nodes[edge.head_micro_node_id.as_str()];
                let tail = nodes[edge.tail_micro_node_id.as_str()];
                assert!(
                    codegraph_core::mvp4_micro_edge_endpoint_pairs(edge.micro_edge_kind)
                        .iter()
                        .any(|pair| pair.head == head.node_kind && pair.tail == tail.node_kind)
                );
                assert!(
                    codegraph_core::mvp4_micro_edge_allowed_derivations(edge.micro_edge_kind)
                        .contains(&edge.provenance.derivation_kind)
                );
                codegraph_core::validate_micro_fact_provenance(&edge.provenance)
                    .unwrap_or_else(|error| panic!("{language:?}: {error}: {edge:#?}"));
                if edge.micro_edge_kind == MicroEdgeKind::LocalMutates {
                    mutated_names.insert(tail.name_or_literal.as_str());
                }
                if edge.micro_edge_kind == MicroEdgeKind::LocalGuards {
                    assert!(edge
                        .provenance
                        .source_fact_ids
                        .contains(&edge.tail_micro_node_id));
                }
            }
            let expected_mutated_names = if *language == SourceLanguage::Ruby {
                BTreeSet::from(["clean"])
            } else {
                BTreeSet::from(["clean", "count"])
            };
            assert_eq!(
                mutated_names, expected_mutated_names,
                "{language:?}: declaration assignments must not mutate: {report:#?}"
            );
        }
    }

    #[test]
    fn derived_connectivity_and_smallest_branch_guards_exclude_siblings() {
        let source = "def run(input):\n    copied = input\n    if copied:\n        copied = input\n        return copied\n    else:\n        copied = input\n        return input\n";
        let report = extract_source(
            SourceLanguage::Python,
            "src/parser-facts-v1/derived_paths.py",
            source,
        );
        let nodes = report
            .nodes
            .iter()
            .map(|node| (node.micro_node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let find_node = |kind, name: &str, line| {
            report
                .nodes
                .iter()
                .find(|node| {
                    node.node_kind == kind
                        && node.name_or_literal == name
                        && node.source_span.start_line == line
                })
                .unwrap_or_else(|| panic!("missing {kind:?} {name} line {line}: {report:#?}"))
        };
        let copied_binding = report
            .nodes
            .iter()
            .find(|node| {
                node.node_kind == MicroNodeKind::LocalBinding && node.name_or_literal == "copied"
            })
            .expect("copied binding");
        let initializer_input = find_node(MicroNodeKind::ValueUse, "input", 2);
        let returned_copy = find_node(MicroNodeKind::ValueUse, "copied", 5);
        let then_return = find_node(MicroNodeKind::ReturnSite, "return", 5);
        let else_return = find_node(MicroNodeKind::ReturnSite, "return", 8);
        assert!(report.edges.iter().any(|edge| {
            edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                && edge.head_micro_node_id == initializer_input.micro_node_id
                && edge.tail_micro_node_id == copied_binding.micro_node_id
        }));
        assert!(report.edges.iter().any(|edge| {
            edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                && edge.head_micro_node_id == returned_copy.micro_node_id
                && edge.tail_micro_node_id == then_return.micro_node_id
        }));

        let mutation_lines = report
            .edges
            .iter()
            .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalMutates)
            .map(|edge| {
                nodes[edge.head_micro_node_id.as_str()]
                    .source_span
                    .start_line
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(mutation_lines, BTreeSet::from([4, 7]));

        let guards_for = |operation_id: &str| {
            report
                .edges
                .iter()
                .filter(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalGuards
                        && edge.tail_micro_node_id == operation_id
                })
                .collect::<Vec<_>>()
        };
        let then_guards = guards_for(&then_return.micro_node_id);
        let else_guards = guards_for(&else_return.micro_node_id);
        assert_eq!(then_guards.len(), 1, "{report:#?}");
        assert_eq!(else_guards.len(), 1, "{report:#?}");
        assert_ne!(
            then_guards[0].head_micro_node_id, else_guards[0].head_micro_node_id,
            "sibling early returns must retain distinct branch paths"
        );
        let then_arm = nodes[then_guards[0].head_micro_node_id.as_str()];
        let else_arm = nodes[else_guards[0].head_micro_node_id.as_str()];
        assert!(then_arm.source_span.end_line < else_return.source_span.start_line);
        assert!(else_arm.source_span.start_line > then_return.source_span.end_line);
    }

    #[test]
    fn compact_branch_preserves_coincident_provenance_roles() {
        let source = "function run(input) { if (input) return input; return input; }";
        let report = extract_source(
            SourceLanguage::JavaScript,
            "src/parser-facts-v1/compact-arm.js",
            source,
        );
        let nodes = report
            .nodes
            .iter()
            .map(|node| (node.micro_node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let edge = report
            .edges
            .iter()
            .find(|edge| {
                edge.micro_edge_kind == MicroEdgeKind::LocalGuards
                    && nodes[edge.tail_micro_node_id.as_str()].node_kind
                        == MicroNodeKind::ReturnSite
            })
            .unwrap_or_else(|| panic!("missing compact-arm guard: {report:#?}"));
        assert_eq!(edge.head_source_span, edge.tail_source_span);
        assert_eq!(edge.relation_source_span, edge.head_source_span);
        assert!(edge.provenance.source_spans.len() >= 3);
        assert_eq!(edge.provenance.source_spans[0], edge.relation_source_span);
        assert_eq!(edge.provenance.source_spans[1], edge.head_source_span);
        assert_eq!(edge.provenance.source_spans[2], edge.tail_source_span);
        assert!(edge
            .provenance
            .source_fact_ids
            .contains(&role_tagged_fact_id(
                "controlling_branch_arm",
                &edge.head_micro_node_id,
            )));
        assert!(edge
            .provenance
            .source_fact_ids
            .contains(&role_tagged_fact_id(
                "guarded_operation",
                &edge.tail_micro_node_id,
            )));
    }

    #[test]
    fn declaration_initializers_write_but_only_later_assignments_mutate() {
        let cases: &[(SourceLanguage, &str, &str, &[u32], &[u32])] = &[
            (
                SourceLanguage::Rust,
                "src/parser-facts-v1/declarations.rs",
                "fn run(input: i32) -> i32 {\n    let mut value = input;\n    let sibling = 0;\n    value = input;\n    return value;\n}\n",
                &[2, 3, 4],
                &[4],
            ),
            (
                SourceLanguage::Python,
                "src/parser-facts-v1/declarations.py",
                "def run(input):\n    value = input\n    value = input\n    return value\n",
                &[2, 3],
                &[3],
            ),
            (
                SourceLanguage::Ruby,
                "src/parser-facts-v1/declarations.rb",
                "def run(input)\n  value = input\n  value = input\n  return value\nend\n",
                &[2, 3],
                &[3],
            ),
            (
                SourceLanguage::Php,
                "src/parser-facts-v1/declarations.php",
                "<?php\nfunction run($input) {\n  $value = $input;\n  $value = $input;\n  return $value;\n}\n",
                &[3, 4],
                &[4],
            ),
        ];
        for (language, path, source, expected_write_lines, expected_mutation_lines) in cases {
            let report = extract_source(*language, path, source);
            let write_lines = report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalWrites)
                .map(|edge| edge.relation_source_span.start_line)
                .collect::<BTreeSet<_>>();
            let mutation_lines = report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalMutates)
                .map(|edge| edge.relation_source_span.start_line)
                .collect::<BTreeSet<_>>();
            assert_eq!(
                write_lines,
                expected_write_lines.iter().copied().collect(),
                "{language:?}: declaration and later writes: {report:#?}"
            );
            assert_eq!(
                mutation_lines,
                expected_mutation_lines.iter().copied().collect(),
                "{language:?}: only later assignments mutate: {report:#?}"
            );
        }
    }

    #[test]
    fn c_family_scalar_writes_mutate_but_indirect_lvalues_never_target_base_bindings() {
        let cases = [
            (
                SourceLanguage::C,
                "src/parser-facts-v1/c-indirect-lvalues.c",
                "struct Record { int field; };\nint run(int input, struct Record object, int *pointer) {\n  int value = input;\n  int array[2];\n  value = input;\n  object.field = input;\n  array[0] = input;\n  *pointer = input;\n  return value;\n}\n",
            ),
            (
                SourceLanguage::Cpp,
                "src/parser-facts-v1/cpp-indirect-lvalues.cpp",
                "struct Record { int field; };\nint run(int input, Record object, int *pointer) {\n  int value = input;\n  int array[2];\n  value = input;\n  object.field = input;\n  array[0] = input;\n  *pointer = input;\n  return value;\n}\n",
            ),
        ];
        for (language, path, source) in cases {
            let report = extract_source(language, path, source);
            let nodes = report
                .nodes
                .iter()
                .map(|node| (node.micro_node_id.as_str(), node))
                .collect::<BTreeMap<_, _>>();
            let write_targets = report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalWrites)
                .map(|edge| {
                    nodes[edge.tail_micro_node_id.as_str()]
                        .name_or_literal
                        .as_str()
                })
                .collect::<Vec<_>>();
            let mutation_targets = report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalMutates)
                .map(|edge| {
                    nodes[edge.tail_micro_node_id.as_str()]
                        .name_or_literal
                        .as_str()
                })
                .collect::<Vec<_>>();
            assert_eq!(
                write_targets
                    .iter()
                    .filter(|name| **name == "value")
                    .count(),
                2,
                "{language:?}: {report:#?}"
            );
            assert_eq!(
                mutation_targets
                    .iter()
                    .filter(|name| **name == "value")
                    .count(),
                1,
                "{language:?}: {report:#?}"
            );
            for base in ["object", "array", "pointer"] {
                assert!(
                    !write_targets.contains(&base) && !mutation_targets.contains(&base),
                    "{language:?}: indirect lvalue resolved to base `{base}`: {report:#?}"
                );
            }
            assert!(
                report
                    .gaps
                    .iter()
                    .filter(|gap| gap.gap_kind == "unsupported_indirect_lvalue")
                    .count()
                    >= 3,
                "{language:?}: {report:#?}"
            );
        }
    }

    #[test]
    fn c_family_pointer_reference_and_alias_sources_never_become_value_flow_proof() {
        let cases = [
            (
                SourceLanguage::C,
                "src/parser-facts-v1/c-pointer-flow.c",
                "int run(int input, int *pointer) {\n  int *alias = &input;\n  int value = *pointer;\n  return *alias;\n}\n",
                &["alias", "value"][..],
            ),
            (
                SourceLanguage::Cpp,
                "src/parser-facts-v1/cpp-pointer-flow.cpp",
                "int run(int input, int *pointer) {\n  int *alias = &input;\n  int &reference = input;\n  int value = *pointer;\n  return reference;\n}\n",
                &["alias", "reference", "value"][..],
            ),
        ];
        for (language, path, source, forbidden_flow_targets) in cases {
            let report = extract_source(language, path, source);
            let nodes = report
                .nodes
                .iter()
                .map(|node| (node.micro_node_id.as_str(), node))
                .collect::<BTreeMap<_, _>>();
            assert!(
                report.edges.iter().all(|edge| {
                    edge.micro_edge_kind != MicroEdgeKind::LocalFlowsTo
                        || !forbidden_flow_targets.contains(
                            &nodes[edge.tail_micro_node_id.as_str()]
                                .name_or_literal
                                .as_str(),
                        )
                }),
                "{language:?}: {report:#?}"
            );
            assert!(
                report.nodes.iter().all(|node| {
                    node.node_kind != MicroNodeKind::ValueUse
                        || !["pointer", "alias", "reference"]
                            .contains(&node.name_or_literal.as_str())
                }),
                "{language:?}: {report:#?}"
            );
            assert!(
                report.gaps.iter().any(|gap| {
                    matches!(
                        gap.gap_kind.as_str(),
                        "unsupported_pointer_value_flow"
                            | "unsupported_pointer_or_reference_alias"
                            | "unsupported_pointer_or_reference_value"
                    )
                }),
                "{language:?}: {report:#?}"
            );
        }
    }

    #[test]
    fn c_family_preprocessor_and_template_regions_are_nonclaimable_but_includes_are_safe() {
        let cases = [
            (
                SourceLanguage::C,
                "src/parser-facts-v1/c-preprocessor.c",
                "#include <stddef.h>\n#define DECLARE_BODY int macro_region(int input) { return input; }\n#if FEATURE_FLAG\nint conditional(int input) { return input; }\n#endif\nint safe(int input) { int value = input; return value; }\n",
                "conditional",
            ),
            (
                SourceLanguage::Cpp,
                "src/parser-facts-v1/cpp-template.cpp",
                "#include <cstddef>\ntemplate <typename T> T templated(T input) { return input; }\n#if FEATURE_FLAG\nint conditional(int input) { return input; }\n#endif\nint safe(int input) { int value = input; return value; }\n",
                "templated",
            ),
        ];
        for (language, path, source, excluded_name) in cases {
            let report = extract_source(language, path, source);
            let frames = report
                .nodes
                .iter()
                .filter(|node| node.node_kind == MicroNodeKind::FunctionFrame)
                .map(|node| node.name_or_literal.as_str())
                .collect::<BTreeSet<_>>();
            assert!(frames.contains("safe"), "{language:?}: {report:#?}");
            assert!(!frames.contains(excluded_name), "{language:?}: {report:#?}");
            assert!(!frames.contains("conditional"), "{language:?}: {report:#?}");
            assert!(
                !frames.contains("macro_region"),
                "{language:?}: {report:#?}"
            );
            assert!(
                report.edges.iter().any(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                        && edge.relation_source_span.start_line == 6
                }),
                "ordinary include must not invalidate safe flow: {language:?}: {report:#?}"
            );
            assert!(
                report
                    .gaps
                    .iter()
                    .any(|gap| { gap.gap_kind == "unsupported_c_family_claimability_region" }),
                "{language:?}: {report:#?}"
            );
        }
    }

    #[test]
    fn c_family_local_calls_require_unique_safe_free_function_definitions() {
        for (language, path, source) in [
            (
                SourceLanguage::C,
                "src/parser-facts-v1/c-prototype-definition.c",
                "int helper(int input);\nint helper(int input) { return input; }\nint run(int input) { return helper(input); }\n",
            ),
            (
                SourceLanguage::Cpp,
                "src/parser-facts-v1/cpp-prototype-definition.cpp",
                "int helper(int input);\nint helper(int input) { return input; }\nint run(int input) { return helper(input); }\n",
            ),
        ] {
            let report = extract_source(language, path, source);
            let frame_names = report
                .nodes
                .iter()
                .filter(|node| node.node_kind == MicroNodeKind::FunctionFrame)
                .map(|node| node.name_or_literal.as_str())
                .collect::<Vec<_>>();
            assert_eq!(frame_names.len(), 2, "{language:?}: {report:#?}");
            assert_eq!(
                report
                    .edges
                    .iter()
                    .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalCalls)
                    .count(),
                1,
                "{language:?}: {report:#?}"
            );
        }

        let negative = extract_source(
            SourceLanguage::Cpp,
            "src/parser-facts-v1/cpp-call-domain-negative.cpp",
            "namespace N { struct Box {}; int adl(Box value) { return 1; } int qualified(int input) { return input; } }\nstruct Box { Box(int); int method(int); int operator()(int); };\nint overloaded(int input) { return input; }\nlong overloaded(long input) { return input; }\nint MACRO_STYLE(int input) { return input; }\nint run(int input, int (*callback)(int), Box box, N::Box namespaced) {\n  callback(input);\n  N::qualified(input);\n  box.method(input);\n  Box(input);\n  box(input);\n  overloaded(input);\n  adl(namespaced);\n  MACRO_STYLE(input);\n  return input;\n}\n",
        );
        assert!(
            negative
                .edges
                .iter()
                .all(|edge| edge.micro_edge_kind != MicroEdgeKind::LocalCalls),
            "{negative:#?}"
        );
        assert!(
            negative
                .gaps
                .iter()
                .any(|gap| gap.gap_kind == "unresolved_direct_call"),
            "{negative:#?}"
        );
    }

    fn normalized_names_by_id(report: &ParserFactsV1Report) -> BTreeMap<String, String> {
        report
            .nodes
            .iter()
            .map(|node| {
                (
                    node.micro_node_id.clone(),
                    normalize_binding_name(&node.name_or_literal),
                )
            })
            .collect()
    }

    fn semantic_edge_kind(kind: MicroEdgeKind) -> bool {
        matches!(
            kind,
            MicroEdgeKind::LocalCalls
                | MicroEdgeKind::LocalFlowsTo
                | MicroEdgeKind::LocalReads
                | MicroEdgeKind::LocalWrites
                | MicroEdgeKind::LocalMutates
        )
    }

    fn assert_gap_kinds(report: &ParserFactsV1Report, expected: &[&str]) {
        for gap_kind in expected {
            assert!(
                report.gaps.iter().any(|gap| gap.gap_kind == *gap_kind),
                "missing gap `{gap_kind}`: {report:#?}"
            );
        }
    }

    fn assert_no_semantic_edges_touching_names(
        report: &ParserFactsV1Report,
        forbidden_names: &[&str],
    ) {
        let names = normalized_names_by_id(report);
        let forbidden = forbidden_names.iter().copied().collect::<BTreeSet<_>>();
        for edge in report
            .edges
            .iter()
            .filter(|edge| semantic_edge_kind(edge.micro_edge_kind))
        {
            let head = names
                .get(&edge.head_micro_node_id)
                .map(String::as_str)
                .unwrap_or("");
            let tail = names
                .get(&edge.tail_micro_node_id)
                .map(String::as_str)
                .unwrap_or("");
            assert!(
                !forbidden.contains(head) && !forbidden.contains(tail),
                "forbidden semantic edge touches `{head}` -> `{tail}`: {report:#?}"
            );
        }
    }

    fn assert_no_local_flow_targets(report: &ParserFactsV1Report, forbidden_targets: &[&str]) {
        let names = normalized_names_by_id(report);
        let forbidden = forbidden_targets.iter().copied().collect::<BTreeSet<_>>();
        for edge in report
            .edges
            .iter()
            .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo)
        {
            let tail = names
                .get(&edge.tail_micro_node_id)
                .map(String::as_str)
                .unwrap_or("");
            assert!(
                !forbidden.contains(tail),
                "forbidden local-flow target `{tail}`: {report:#?}"
            );
        }
    }

    fn assert_has_local_flow_target(report: &ParserFactsV1Report, expected_target: &str) {
        let names = normalized_names_by_id(report);
        assert!(
            report.edges.iter().any(|edge| {
                edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                    && names
                        .get(&edge.tail_micro_node_id)
                        .is_some_and(|name| name == expected_target)
            }),
            "missing local-flow target `{expected_target}`: {report:#?}"
        );
    }

    fn assert_no_write_or_mutate_targets(report: &ParserFactsV1Report, forbidden_targets: &[&str]) {
        let names = normalized_names_by_id(report);
        let forbidden = forbidden_targets.iter().copied().collect::<BTreeSet<_>>();
        for edge in report.edges.iter().filter(|edge| {
            matches!(
                edge.micro_edge_kind,
                MicroEdgeKind::LocalWrites | MicroEdgeKind::LocalMutates
            )
        }) {
            let tail = names
                .get(&edge.tail_micro_node_id)
                .map(String::as_str)
                .unwrap_or("");
            assert!(
                !forbidden.contains(tail),
                "forbidden write/mutate target `{tail}`: {report:#?}"
            );
        }
    }

    fn assert_no_callsite_result_flows(report: &ParserFactsV1Report) {
        let nodes = report
            .nodes
            .iter()
            .map(|node| (node.micro_node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        assert!(
            report.edges.iter().all(|edge| {
                edge.micro_edge_kind != MicroEdgeKind::LocalFlowsTo
                    || nodes
                        .get(edge.head_micro_node_id.as_str())
                        .is_none_or(|node| node.node_kind != MicroNodeKind::CallSite)
            }),
            "forbidden callsite result producer: {report:#?}"
        );
    }

    fn semantic_signature_for_name(report: &ParserFactsV1Report, name: &str) -> Vec<String> {
        let names = normalized_names_by_id(report);
        let mut signature = report
            .edges
            .iter()
            .filter(|edge| semantic_edge_kind(edge.micro_edge_kind))
            .filter_map(|edge| {
                let head = names.get(&edge.head_micro_node_id)?;
                let tail = names.get(&edge.tail_micro_node_id)?;
                ((head == name) || (tail == name)).then(|| {
                    format!(
                        "{:?}|{}|{}|{}",
                        edge.micro_edge_kind, head, tail, edge.relation_source_span.start_line
                    )
                })
            })
            .collect::<Vec<_>>();
        signature.sort();
        signature
    }

    fn assert_unrelated_local_is_stable(
        report: &ParserFactsV1Report,
        replay: &ParserFactsV1Report,
        name: &str,
    ) {
        let signature = semantic_signature_for_name(report, name);
        assert!(!signature.is_empty(), "missing `{name}` path: {report:#?}");
        assert!(
            signature
                .iter()
                .any(|entry| entry.starts_with("LocalFlowsTo|")),
            "`{name}` has no local flow: {report:#?}"
        );
        assert_eq!(signature, semantic_signature_for_name(replay, name));
    }

    fn frame_names(report: &ParserFactsV1Report) -> BTreeSet<String> {
        report
            .nodes
            .iter()
            .filter(|node| node.node_kind == MicroNodeKind::FunctionFrame)
            .map(|node| normalize_binding_name(&node.name_or_literal))
            .collect()
    }

    fn return_site_lines(report: &ParserFactsV1Report) -> BTreeSet<String> {
        report
            .nodes
            .iter()
            .filter(|node| node.node_kind == MicroNodeKind::ReturnSite)
            .map(|node| node.source_span.start_line.to_string())
            .collect()
    }

    fn assert_exact_return_site_flow(
        report: &ParserFactsV1Report,
        line: &str,
        producer_kind: MicroNodeKind,
    ) {
        let nodes = report
            .nodes
            .iter()
            .map(|node| (node.micro_node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let return_sites = report
            .nodes
            .iter()
            .filter(|node| {
                node.node_kind == MicroNodeKind::ReturnSite
                    && node.source_span.start_line.to_string() == line
            })
            .collect::<Vec<_>>();
        assert_eq!(return_sites.len(), 1, "line {line}: {report:#?}");
        let return_site_id = return_sites[0].micro_node_id.as_str();
        assert!(
            report.edges.iter().any(|edge| {
                edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                    && edge.tail_micro_node_id == return_site_id
                    && nodes
                        .get(edge.head_micro_node_id.as_str())
                        .is_some_and(|node| node.node_kind == producer_kind)
            }),
            "line {line} missing {producer_kind:?} flow into ReturnSite: {report:#?}"
        );
    }

    fn assert_no_return_flow_lines(report: &ParserFactsV1Report, forbidden_lines: &[&str]) {
        let nodes = report
            .nodes
            .iter()
            .map(|node| (node.micro_node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let forbidden = forbidden_lines.iter().copied().collect::<BTreeSet<_>>();
        for edge in report
            .edges
            .iter()
            .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo)
        {
            let Some(target) = nodes.get(edge.tail_micro_node_id.as_str()) else {
                continue;
            };
            assert!(
                target.node_kind != MicroNodeKind::ReturnSite
                    || !forbidden.contains(target.source_span.start_line.to_string().as_str()),
                "forbidden implicit-return flow: {report:#?}"
            );
        }
    }

    #[test]
    fn ruby_implicit_returns_and_modifiers_preserve_distinct_paths() {
        let path = "src/parser-facts-v1/ruby-implicit-returns.rb";
        let source = r#"def helper(input)
  stable = input
  stable
end
def literal_tail
  7
end
def call_tail(input)
  helper(input)
end
def explicit_tail(input)
  return input
end
def modified(input, flag)
  affected = input
  affected if flag
end
def array_tail(input)
  [input]
end
"#;
        let report = extract_source(SourceLanguage::Ruby, path, source);
        let replay = extract_source(SourceLanguage::Ruby, path, source);
        assert_gap_kinds(
            &report,
            &[
                "unsupported_ruby_modifier_flow",
                "unsupported_ruby_implicit_return_shape",
            ],
        );
        assert_eq!(
            return_site_lines(&report),
            ["3", "6", "9", "12"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            "{report:#?}"
        );
        assert_exact_return_site_flow(&report, "3", MicroNodeKind::ValueUse);
        assert_exact_return_site_flow(&report, "9", MicroNodeKind::CallSite);
        assert_exact_return_site_flow(&report, "12", MicroNodeKind::ValueUse);
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalCalls)
                .count(),
            1,
            "{report:#?}"
        );
        assert_no_return_flow_lines(&report, &["16", "19"]);
        assert_unrelated_local_is_stable(&report, &replay, "stable");
    }

    #[test]
    fn ruby_blocks_lambdas_and_nonlocal_bindings_fail_closed() {
        let path = "src/parser-facts-v1/ruby-ownership-boundaries.rb";
        let source = r#"class OpenClass
  def class_method(input)
    input
  end
end
module OpenModule
  def module_method(input)
    input
  end
end
def self.singleton_method(input)
  input
end
def safe(input)
  stable = input
  values = [input]
  values.each do |block_value|
    escaped = block_value
    return escaped
  end
  callback = ->(lambda_value) { lambda_value }
  yield input
  @instance_value = input
  @@class_value = input
  $global_value = input
  for loop_value in values
    loop_copy = loop_value
  end
  input => pattern_value
  case input
  in Integer
    matched = input
  end
  stable
end
"#;
        let report = extract_source(SourceLanguage::Ruby, path, source);
        let replay = extract_source(SourceLanguage::Ruby, path, source);
        assert_gap_kinds(
            &report,
            &[
                "unsupported_ruby_function_domain",
                "unsupported_ruby_closure_boundary",
                "unsupported_ruby_nonlocal_control_flow",
                "unsupported_ruby_nonlocal_binding",
                "unsupported_ruby_for_binding",
                "unsupported_ruby_pattern_binding",
                "unsupported_ruby_complex_control_flow",
            ],
        );
        assert_eq!(
            frame_names(&report),
            ["safe".to_string()].into_iter().collect()
        );
        assert_eq!(
            return_site_lines(&report),
            ["34"].into_iter().map(str::to_string).collect(),
            "{report:#?}"
        );
        assert_no_semantic_edges_touching_names(
            &report,
            &[
                "block_value",
                "escaped",
                "lambda_value",
                "instance_value",
                "class_value",
                "global_value",
                "loop_value",
                "loop_copy",
                "pattern_value",
                "matched",
            ],
        );
        assert_unrelated_local_is_stable(&report, &replay, "stable");
    }

    #[test]
    fn ruby_local_calls_require_unique_plain_static_method_domain() {
        let path = "src/parser-facts-v1/ruby-local-call-domain.rb";
        let source = r#"def helper(input)
  input
end
def duplicate(input)
  input
end
def duplicate(input)
  input
end
def run(input)
  stable = input
  positive = helper(input)
  ambiguous_result = duplicate(input)
  receiver_result = self.helper(input)
  block_result = helper(input) { |value| value }
  command_result = helper input
  spaced_command_result = helper (input)
  stable
end
"#;
        let report = extract_source(SourceLanguage::Ruby, path, source);
        let replay = extract_source(SourceLanguage::Ruby, path, source);
        assert_gap_kinds(
            &report,
            &[
                "unresolved_direct_call",
                "unsupported_ruby_closure_boundary",
            ],
        );
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalCalls)
                .count(),
            1,
            "{report:#?}"
        );
        assert_has_local_flow_target(&report, "positive");
        assert_no_local_flow_targets(
            &report,
            &[
                "ambiguous_result",
                "receiver_result",
                "block_result",
                "command_result",
                "spaced_command_result",
            ],
        );
        assert_unrelated_local_is_stable(&report, &replay, "stable");
    }

    #[test]
    fn ruby_metaprogramming_open_classes_and_dynamic_send_fail_closed() {
        let variants = [
            ("alias", "alias helper_alias helper"),
            ("undef", "undef helper"),
            ("alias_method", "alias_method :helper_alias, :helper"),
            ("define_method", "define_method(:dynamic) { |value| value }"),
            ("method_missing", "def method_missing(value)\n  value\nend"),
            ("send", "send(:helper, 1)"),
            ("public_send", "public_send(:helper, 1)"),
            ("eval", "eval(\"1\")"),
            ("prepend", "prepend SomeModule"),
            ("include", "include SomeModule"),
            ("extend", "extend SomeModule"),
            ("remove_method", "remove_method :helper"),
            ("undef_method", "undef_method :helper"),
            ("require", "require \"plugin\""),
            ("require_relative", "require_relative \"plugin\""),
            ("load", "load \"plugin.rb\""),
            ("autoload", "autoload :Thing, \"thing\""),
            ("class_eval", "class_eval(\"1\")"),
            ("module_eval", "module_eval(\"1\")"),
            ("instance_eval", "instance_eval(\"1\")"),
            ("class_exec", "class_exec { 1 }"),
            ("module_exec", "module_exec { 1 }"),
            ("instance_exec", "instance_exec { 1 }"),
            (
                "define_singleton_method",
                "define_singleton_method(:dynamic) { |value| value }",
            ),
            ("refine", "refine String do\nend"),
            ("using", "using SomeRefinement"),
            ("module_function", "module_function :helper"),
            ("open_class", "class OpenClass\nend"),
            ("open_module", "module OpenModule\nend"),
            ("singleton_class", "class << self\nend"),
            (
                "singleton_method",
                "def self.singleton_helper(value)\n  value\nend",
            ),
            (
                "nested_def",
                "def container(value)\n  def nested_helper(inner)\n    inner\n  end\n  value\nend",
            ),
            (
                "conditional_def",
                "if true\n  def conditional_helper(value)\n    value\n  end\nend",
            ),
        ];
        for (label, hazard) in variants {
            let path = format!("src/parser-facts-v1/ruby-hazard-{label}.rb");
            let source = format!(
                "def helper(input)\n  input\nend\n{hazard}\ndef run(input)\n  stable = input\n  poisoned = helper(input)\n  stable\nend\n"
            );
            let report = extract_source(SourceLanguage::Ruby, &path, &source);
            let replay = extract_source(SourceLanguage::Ruby, &path, &source);
            assert_gap_kinds(&report, &["unsupported_ruby_file_resolution_domain"]);
            assert_eq!(
                report
                    .edges
                    .iter()
                    .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalCalls)
                    .count(),
                0,
                "{label}: {report:#?}"
            );
            assert_no_local_flow_targets(&report, &["poisoned"]);
            assert_no_callsite_result_flows(&report);
            let frames = frame_names(&report);
            assert!(
                frames.contains("helper") && frames.contains("run"),
                "{label}: {report:#?}"
            );
            assert_unrelated_local_is_stable(&report, &replay, "stable");
        }
    }

    #[test]
    fn php_dynamic_calls_includes_and_variable_variables_never_flow() {
        let path = "src/parser-facts-v1/php-dynamic-resolution.php";
        let source = r#"<?php
namespace App;
use function Vendor\remote as remote_alias;
use Vendor\Package\{function grouped as grouped_alias};
function real($input) { return $input; }
function run($input) {
  $stable = $input;
  $dynamic = $fn($input);
  $variable = ${$unknown};
  include 'one.php';
  include_once 'two.php';
  require 'three.php';
  require_once 'four.php';
  eval('$value = 1;');
  $poisoned = real($input);
  return $stable;
}
"#;
        let report = extract_source(SourceLanguage::Php, path, source);
        let replay = extract_source(SourceLanguage::Php, path, source);
        assert_gap_kinds(
            &report,
            &[
                "unsupported_php_dynamic_variable",
                "unsupported_php_dynamic_include",
                "unsupported_php_file_resolution_domain",
                "unresolved_direct_call",
            ],
        );
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalCalls)
                .count(),
            0,
            "{report:#?}"
        );
        assert_no_semantic_edges_touching_names(&report, &["fn", "unknown"]);
        assert_no_local_flow_targets(&report, &["dynamic", "variable", "poisoned"]);
        assert_no_callsite_result_flows(&report);
        assert_unrelated_local_is_stable(&report, &replay, "stable");

        let clean_source = r#"<?php
namespace App;
function helper($input) { return $input; }
function clean_run($input, $helper) {
  $stable_clean = $input;
  $dynamic_result = $helper($input);
  $real_result = helper($input);
  return $stable_clean;
}
"#;
        let clean_report = extract_source(
            SourceLanguage::Php,
            "src/parser-facts-v1/php-dynamic-call-clean.php",
            clean_source,
        );
        assert_eq!(
            clean_report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalCalls)
                .count(),
            1,
            "{clean_report:#?}"
        );
        assert_eq!(
            clean_report
                .nodes
                .iter()
                .filter(|node| node.node_kind == MicroNodeKind::CallSite)
                .map(|node| node.source_span.start_line.to_string())
                .collect::<BTreeSet<_>>(),
            ["7"].into_iter().map(str::to_string).collect(),
            "{clean_report:#?}"
        );
        assert_has_local_flow_target(&clean_report, "real_result");
        assert_no_local_flow_targets(&clean_report, &["dynamic_result"]);

        let hazards = [
            (
                "use_function",
                "use function Vendor\\remote as remote_alias;",
                "",
            ),
            (
                "group_use_function",
                "use Vendor\\Package\\{function grouped as grouped_alias};",
                "",
            ),
            ("include", "", "include 'one.php';"),
            ("include_once", "", "include_once 'two.php';"),
            ("require", "", "require 'three.php';"),
            ("require_once", "", "require_once 'four.php';"),
            ("eval", "", "eval('$value = 1;');"),
        ];
        for (label, prelude, body_hazard) in hazards {
            let variant_path = format!("src/parser-facts-v1/php-hazard-{label}.php");
            let variant_source = format!(
                "<?php\nnamespace App;\n{prelude}\nfunction real($input) {{ return $input; }}\nfunction run($input) {{\n  $stable = $input;\n  {body_hazard}\n  $poisoned = real($input);\n  return $stable;\n}}\n"
            );
            let variant_report =
                extract_source(SourceLanguage::Php, &variant_path, &variant_source);
            let variant_replay =
                extract_source(SourceLanguage::Php, &variant_path, &variant_source);
            assert_gap_kinds(&variant_report, &["unsupported_php_file_resolution_domain"]);
            assert_eq!(
                variant_report
                    .edges
                    .iter()
                    .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalCalls)
                    .count(),
                0,
                "{label}: {variant_report:#?}"
            );
            assert_no_local_flow_targets(&variant_report, &["poisoned"]);
            assert_no_callsite_result_flows(&variant_report);
            assert_unrelated_local_is_stable(&variant_report, &variant_replay, "stable");
        }
    }

    #[test]
    fn php_closures_references_globals_and_statics_fail_closed() {
        let path = "src/parser-facts-v1/php-reference-state.php";
        let source = r#"<?php
function &ref_return($input) {
  return $input;
}
function generator_yield($input) {
  yield $input;
}
function generator_from($input) {
  yield from [$input];
}
function stable_run($input, &$byref_param) {
  $stable = $input;
  global $global_name;
  static $first = 1, $second = 2;
  $alias =& $byref_param;
  foreach ([$input] as &$item) {
    $item = $input;
  }
  $capture = $input;
  $closure = function () use (&$capture) { return $capture; };
  $arrow = fn($value) => $value;
  $byref_param = 3;
  $unsafe_byref_param = $byref_param;
  $global_name = 3;
  $unsafe_global = $global_name;
  $first = 3;
  $unsafe_first = $first;
  $second = 3;
  $unsafe_second = $second;
  $alias = 3;
  $unsafe_alias = $alias;
  $item = 3;
  $unsafe_item = $item;
  $capture = 3;
  $unsafe_capture = $capture;
  $ref_result = ref_return($stable);
  $yield_result = generator_yield($stable);
  $yield_from_result = generator_from($stable);
  return $stable;
}
function nested_yield_owner($input) {
  $nested_stable = $input;
  $closure = function () use ($input) {
    yield $input;
  };
  return $nested_stable;
}
"#;
        let report = extract_source(SourceLanguage::Php, path, source);
        let replay = extract_source(SourceLanguage::Php, path, source);
        assert_gap_kinds(
            &report,
            &[
                "unsupported_php_reference_return_function",
                "unsupported_php_generator_function",
                "unsupported_php_closure_boundary",
                "unsupported_php_reference_semantics",
                "unsupported_php_nonlocal_binding",
                "unsupported_php_nonlocal_or_reference_value",
            ],
        );
        let frames = frame_names(&report);
        assert!(
            frames.contains("stable_run") && frames.contains("nested_yield_owner"),
            "{report:#?}"
        );
        assert!(
            !frames.contains("ref_return")
                && !frames.contains("generator_yield")
                && !frames.contains("generator_from"),
            "{report:#?}"
        );
        assert_no_semantic_edges_touching_names(
            &report,
            &[
                "byref_param",
                "global_name",
                "first",
                "second",
                "alias",
                "item",
                "capture",
            ],
        );
        let forbidden_persistent_names = [
            "byref_param",
            "global_name",
            "first",
            "second",
            "alias",
            "item",
            "capture",
        ];
        for node in report.nodes.iter().filter(|node| {
            matches!(
                node.node_kind,
                MicroNodeKind::Parameter | MicroNodeKind::LocalBinding | MicroNodeKind::ValueUse
            )
        }) {
            let name = normalize_binding_name(&node.name_or_literal);
            assert!(
                !forbidden_persistent_names.contains(&name.as_str()),
                "forbidden persistent node `{name}`: {report:#?}"
            );
        }
        assert_no_local_flow_targets(
            &report,
            &[
                "unsafe_byref_param",
                "unsafe_global",
                "unsafe_first",
                "unsafe_second",
                "unsafe_alias",
                "unsafe_item",
                "unsafe_capture",
                "ref_result",
                "yield_result",
                "yield_from_result",
            ],
        );
        assert_no_callsite_result_flows(&report);
        assert_has_local_flow_target(&report, "nested_stable");
        assert_unrelated_local_is_stable(&report, &replay, "stable");
        assert_unrelated_local_is_stable(&report, &replay, "nested_stable");

        let nested_frame = report
            .nodes
            .iter()
            .find(|node| {
                node.node_kind == MicroNodeKind::FunctionFrame
                    && normalize_binding_name(&node.name_or_literal) == "nested_yield_owner"
            })
            .unwrap_or_else(|| panic!("missing nested_yield_owner frame: {report:#?}"));
        let nested_identity = nested_frame.enclosing_function_identity.as_str();
        let nested_bindings = report
            .nodes
            .iter()
            .filter(|node| {
                node.node_kind == MicroNodeKind::LocalBinding
                    && node.enclosing_function_identity == nested_identity
                    && normalize_binding_name(&node.name_or_literal) == "nested_stable"
            })
            .collect::<Vec<_>>();
        let nested_uses = report
            .nodes
            .iter()
            .filter(|node| {
                node.node_kind == MicroNodeKind::ValueUse
                    && node.enclosing_function_identity == nested_identity
                    && normalize_binding_name(&node.name_or_literal) == "nested_stable"
            })
            .collect::<Vec<_>>();
        let nested_returns = report
            .nodes
            .iter()
            .filter(|node| {
                node.node_kind == MicroNodeKind::ReturnSite
                    && node.enclosing_function_identity == nested_identity
            })
            .collect::<Vec<_>>();
        assert_eq!(nested_bindings.len(), 1, "{report:#?}");
        assert_eq!(nested_uses.len(), 1, "{report:#?}");
        assert_eq!(nested_returns.len(), 1, "{report:#?}");
        let nested_binding_id = nested_bindings[0].micro_node_id.as_str();
        let nested_use_id = nested_uses[0].micro_node_id.as_str();
        let nested_return_id = nested_returns[0].micro_node_id.as_str();
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalWrites
                        && edge.tail_micro_node_id == nested_binding_id
                })
                .count(),
            1,
            "{report:#?}"
        );
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalReads
                        && edge.head_micro_node_id == nested_use_id
                        && edge.tail_micro_node_id == nested_binding_id
                })
                .count(),
            1,
            "{report:#?}"
        );
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                        && edge.tail_micro_node_id == nested_binding_id
                })
                .count(),
            1,
            "{report:#?}"
        );
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                        && edge.head_micro_node_id == nested_use_id
                        && edge.tail_micro_node_id == nested_return_id
                })
                .count(),
            1,
            "{report:#?}"
        );
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalReturnsTo
                        && edge.head_micro_node_id == nested_return_id
                        && edge.tail_micro_node_id == nested_frame.micro_node_id
                })
                .count(),
            1,
            "{report:#?}"
        );
    }

    #[test]
    fn php_local_calls_require_unique_same_namespace_named_functions() {
        let cases = [
            (
                "braced",
                r#"<?php
namespace Vendor\Alpha {
  function Helper($input) { return $input; }
}
namespace vendor\alpha {
  function run($input) {
    $stable = $input;
    $positive = helper($input);
    return $stable;
  }
}
namespace Vendor\Dup {
  function Foo($input) { return $input; }
  function foo($input) { return $input; }
  function ambiguous($input) {
    $stable = $input;
    $ambiguous_result = FOO($input);
    return $stable;
  }
}
namespace Vendor\Other {
  function cross_only($input) {
    $stable = $input;
    $cross_result = Helper($input);
    return $stable;
  }
}
"#,
            ),
            (
                "unbraced",
                r#"<?php
namespace Vendor\Mixed;
function Helper($input) { return $input; }
namespace vendor\mixed;
function run($input) {
  $stable = $input;
  $positive = HELPER($input);
  return $stable;
}
namespace Vendor\Dup;
function Foo($input) { return $input; }
function foo($input) { return $input; }
function ambiguous($input) {
  $stable = $input;
  $ambiguous_result = foo($input);
  return $stable;
}
namespace Vendor\Other;
function cross_only($input) {
  $stable = $input;
  $cross_result = Helper($input);
  return $stable;
}
"#,
            ),
        ];
        for (label, source) in cases {
            let path = format!("src/parser-facts-v1/php-namespace-{label}.php");
            let report = extract_source(SourceLanguage::Php, &path, source);
            let replay = extract_source(SourceLanguage::Php, &path, source);
            assert_gap_kinds(&report, &["unresolved_direct_call"]);
            assert_eq!(
                report
                    .edges
                    .iter()
                    .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalCalls)
                    .count(),
                1,
                "{label}: {report:#?}"
            );
            assert_has_local_flow_target(&report, "positive");
            assert_no_local_flow_targets(&report, &["ambiguous_result", "cross_result"]);
            assert_unrelated_local_is_stable(&report, &replay, "stable");
        }
    }

    #[test]
    fn php_magic_methods_member_calls_and_property_writes_never_prove_targets() {
        let path = "src/parser-facts-v1/php-member-property-domain.php";
        let source = r#"<?php
class Box {
  public function __construct(public $promoted) {}
  public function __get($name) { return $this->$name; }
  public function __call($name, $arguments) { return null; }
  public static function staticMethod($input) { return $input; }
}
function run($input, $object, $name) {
  $stable = $input;
  $member_result = $object->method($input);
  $nullsafe_result = $object?->method($input);
  $scoped_result = Box::staticMethod($input);
  $created_result = new Box($input);
  $invoked_result = $object($input);
  $member_property_result = $object->property;
  $nullsafe_property_result = $object?->property;
  $scoped_property_result = Box::$staticProperty;
  $dynamic_property_result = $object->$name;
  $object->property = $input;
  $object?->property;
  Box::$staticProperty = $input;
  return $stable;
}
"#;
        let report = extract_source(SourceLanguage::Php, path, source);
        let replay = extract_source(SourceLanguage::Php, path, source);
        assert_gap_kinds(
            &report,
            &[
                "unsupported_php_function_domain",
                "unsupported_php_call_domain",
                "unsupported_php_property_flow",
            ],
        );
        assert_eq!(
            frame_names(&report),
            ["run".to_string()].into_iter().collect()
        );
        assert_eq!(
            report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalCalls)
                .count(),
            0,
            "{report:#?}"
        );
        assert_eq!(
            report
                .nodes
                .iter()
                .filter(|node| node.node_kind == MicroNodeKind::CallSite)
                .count(),
            0,
            "{report:#?}"
        );
        assert_no_local_flow_targets(
            &report,
            &[
                "member_result",
                "nullsafe_result",
                "scoped_result",
                "created_result",
                "invoked_result",
                "member_property_result",
                "nullsafe_property_result",
                "scoped_property_result",
                "dynamic_property_result",
            ],
        );
        assert_no_callsite_result_flows(&report);
        assert_no_write_or_mutate_targets(
            &report,
            &["object", "property", "staticProperty", "promoted"],
        );
        assert_no_semantic_edges_touching_names(&report, &["promoted"]);
        assert_unrelated_local_is_stable(&report, &replay, "stable");
    }

    #[test]
    fn marker_gate_covers_all_frontends_with_binding_and_occurrence_proof() {
        for language in SourceLanguage::ALL {
            let report = extract(*language, "src/parser-facts-v1");
            let nodes = report
                .nodes
                .iter()
                .map(|node| (node.micro_node_id.as_str(), node))
                .collect::<BTreeMap<_, _>>();
            let sanitizes = report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalSanitizes)
                .collect::<Vec<_>>();
            let asserts = report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalAsserts)
                .collect::<Vec<_>>();
            assert_eq!(sanitizes.len(), 1, "{language:?}: {report:#?}");
            assert_eq!(asserts.len(), 1, "{language:?}: {report:#?}");
            let sanitize = sanitizes[0];
            let input = nodes[sanitize.head_micro_node_id.as_str()];
            let output = nodes[sanitize.tail_micro_node_id.as_str()];
            assert_eq!(input.node_kind, MicroNodeKind::Parameter);
            assert_eq!(input.name_or_literal.trim_start_matches('$'), "input");
            assert_eq!(output.node_kind, MicroNodeKind::LocalBinding);
            assert_eq!(output.name_or_literal.trim_start_matches('$'), "clean");
            assert_eq!(sanitize.exactness, MicroExactness::DerivedWithProvenance);
            assert!(sanitize
                .provenance
                .source_fact_ids
                .iter()
                .any(|id| id.starts_with("parser_facts_v1_role:sanitizer_input_binding:")));
            assert!(sanitize
                .provenance
                .source_fact_ids
                .iter()
                .any(|id| id.starts_with("parser_facts_v1_role:sanitizer_call:")));
            assert!(
                codegraph_core::mvp4_micro_edge_endpoint_pairs(MicroEdgeKind::LocalSanitizes)
                    .iter()
                    .any(|pair| pair.head == input.node_kind && pair.tail == output.node_kind)
            );
            codegraph_core::validate_micro_fact_provenance(&sanitize.provenance)
                .unwrap_or_else(|error| panic!("{language:?}: {error}: {sanitize:#?}"));

            let sanitizer_result_flow = report
                .edges
                .iter()
                .find(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                        && nodes[edge.head_micro_node_id.as_str()].node_kind
                            == MicroNodeKind::SanitizerCall
                        && edge.tail_micro_node_id == sanitize.tail_micro_node_id
                })
                .unwrap_or_else(|| {
                    panic!(
                        "{language:?}: sanitizer result must flow to output binding: {report:#?}"
                    )
                });
            let sanitizer_call_id = sanitizer_result_flow.head_micro_node_id.as_str();
            let sanitizer_input_flow = report
                .edges
                .iter()
                .find(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                        && edge.tail_micro_node_id == sanitizer_call_id
                        && nodes[edge.head_micro_node_id.as_str()].node_kind
                            == MicroNodeKind::ValueUse
                        && nodes[edge.head_micro_node_id.as_str()]
                            .name_or_literal
                            .trim_start_matches('$')
                            == "input"
                })
                .unwrap_or_else(|| {
                    panic!(
                "{language:?}: exact argument occurrence must flow into SanitizerCall: {report:#?}"
            )
                });
            let return_flow = report
                .edges
                .iter()
                .find(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo
                        && nodes[edge.head_micro_node_id.as_str()].node_kind
                            == MicroNodeKind::ValueUse
                        && nodes[edge.head_micro_node_id.as_str()]
                            .name_or_literal
                            .trim_start_matches('$')
                            == "clean"
                        && nodes[edge.tail_micro_node_id.as_str()].node_kind
                            == MicroNodeKind::ReturnSite
                })
                .unwrap_or_else(|| {
                    panic!(
                "{language:?}: sanitized output read must continue to ReturnSite: {report:#?}"
            )
                });
            assert!(
                report.edges.iter().any(|edge| {
                    edge.micro_edge_kind == MicroEdgeKind::LocalReads
                        && edge.head_micro_node_id == return_flow.head_micro_node_id
                        && edge.tail_micro_node_id == sanitize.tail_micro_node_id
                }),
                "{language:?}: return read must resolve to sanitized output binding: {report:#?}"
            );
            for edge in [sanitize, sanitizer_input_flow, sanitizer_result_flow] {
                for role in [
                    "codegraph_sanitizer_marker",
                    "marked_sanitizer_helper",
                    "sanitizer_callsite",
                    "sanitizer_input_occurrence",
                    "sanitizer_input_binding",
                    "sanitized_output_binding",
                ] {
                    assert!(
                        edge.provenance
                            .source_fact_ids
                            .iter()
                            .any(|id| { id.starts_with(&format!("parser_facts_v1_role:{role}:")) }),
                        "{language:?}: missing {role} provenance: {edge:#?}"
                    );
                }
                codegraph_core::validate_micro_fact_provenance(&edge.provenance)
                    .unwrap_or_else(|error| panic!("{language:?}: {error}: {edge:#?}"));
            }

            let assertion = asserts[0];
            let asserted_value = nodes[assertion.head_micro_node_id.as_str()];
            let assertion_site = nodes[assertion.tail_micro_node_id.as_str()];
            assert_eq!(asserted_value.node_kind, MicroNodeKind::ValueUse);
            assert_eq!(
                asserted_value.name_or_literal.trim_start_matches('$'),
                "clean"
            );
            assert_eq!(assertion_site.node_kind, MicroNodeKind::TestAssertion);
            assert_eq!(assertion.exactness, MicroExactness::DerivedWithProvenance);
            assert!(assertion
                .provenance
                .source_fact_ids
                .iter()
                .any(|id| id.starts_with("parser_facts_v1_role:marked_assertion_helper:")));
            assert!(assertion
                .provenance
                .source_fact_ids
                .contains(&assertion.head_micro_node_id));
            assert!(assertion
                .provenance
                .source_fact_ids
                .contains(&assertion.tail_micro_node_id));
            codegraph_core::validate_micro_fact_provenance(&assertion.provenance)
                .unwrap_or_else(|error| panic!("{language:?}: {error}: {assertion:#?}"));
            assert!(!report.production_activation_enabled);
        }
    }

    #[test]
    fn native_python_and_java_assertions_are_exact_test_assertion_sites() {
        let cases = [
            (
                SourceLanguage::Python,
                "src/parser-facts-v1/native_assert.py",
                "def run(input):\n    assert input\n    return input\n",
            ),
            (
                SourceLanguage::Java,
                "src/parser-facts-v1/NativeAssert.java",
                "class NativeAssert { static int run(int input) { assert input > 0; return input; } }",
            ),
        ];
        for (language, path, source) in cases {
            let report = extract_source(language, path, source);
            let assertions = report
                .edges
                .iter()
                .filter(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalAsserts)
                .collect::<Vec<_>>();
            assert_eq!(assertions.len(), 1, "{language:?}: {report:#?}");
            let assertion = assertions[0];
            assert!(assertion
                .provenance
                .source_fact_ids
                .iter()
                .any(|id| id.starts_with("parser_facts_v1_role:native_assertion_syntax:")));
            codegraph_core::validate_micro_fact_provenance(&assertion.provenance)
                .unwrap_or_else(|error| panic!("{language:?}: {error}: {assertion:#?}"));
        }
    }

    #[test]
    fn marker_text_names_and_non_direct_resolution_never_authorize_semantics() {
        let cases = [
            (
                "marker-in-string",
                "const a = '@codegraph-sanitizer'; const b = '@codegraph-assertion'; function sanitizeValue(v) { return v; } function assertValue(v) { return v; } function run(input) { let clean = sanitizeValue(input); assertValue(clean); return clean; }",
            ),
            (
                "unattached-comment",
                "// @codegraph-sanitizer\n\nfunction sanitizeValue(v) { return v; }\n// @codegraph-assertion\n\nfunction assertValue(v) { return v; }\nfunction run(input) { let clean = sanitizeValue(input); assertValue(clean); return clean; }",
            ),
            (
                "marker-on-other-helper",
                "// @codegraph-sanitizer\nfunction scrub(v) { return v; }\n// @codegraph-assertion\nfunction check(v) { return v; }\nfunction sanitizeValue(v) { return v; } function assertValue(v) { return v; } function run(input) { let clean = sanitizeValue(input); assertValue(clean); return clean; }",
            ),
            (
                "name-only",
                "function sanitizeValue(v) { return v; } function assertValue(v) { return v; } function run(input) { let clean = sanitizeValue(input); assertValue(clean); return clean; }",
            ),
            (
                "member-and-dynamic",
                "// @codegraph-sanitizer\nfunction sanitizeValue(v) { return v; }\n// @codegraph-assertion\nfunction assertValue(v) { return v; }\nconst holder = { sanitizeValue, assertValue }; function run(input) { let clean = holder.sanitizeValue(input); holder['assertValue'](clean); return clean; }",
            ),
            (
                "ambiguous-callee",
                "// @codegraph-sanitizer\nfunction sanitizeValue(v) { return v; }\n// @codegraph-sanitizer\nfunction sanitizeValue(v) { return v; }\nfunction run(input) { let clean = sanitizeValue(input); return clean; }",
            ),
            (
                "shadowed-callee",
                "// @codegraph-sanitizer\nfunction sanitizeValue(v) { return v; }\n// @codegraph-assertion\nfunction assertValue(v) { return v; }\nfunction run(input, sanitizeValue, assertValue) { let clean = sanitizeValue(input); assertValue(clean); return clean; }",
            ),
        ];
        for (name, source) in cases {
            let path = format!("src/parser-facts-v1/marker-negative-{name}.js");
            let report = extract_source(SourceLanguage::JavaScript, &path, source);
            assert!(
                report.nodes.iter().all(|node| !matches!(
                    node.node_kind,
                    MicroNodeKind::SanitizerCall | MicroNodeKind::TestAssertion
                )),
                "{name}: {report:#?}"
            );
            assert!(
                report.edges.iter().all(|edge| !matches!(
                    edge.micro_edge_kind,
                    MicroEdgeKind::LocalSanitizes | MicroEdgeKind::LocalAsserts
                )),
                "{name}: {report:#?}"
            );
        }

        let recovery = extract_source(
            SourceLanguage::JavaScript,
            "src/parser-facts-v1/marker-recovery.js",
            "// @codegraph-sanitizer\nfunction sanitizeValue(v) { return v; function run(input) { return sanitizeValue(input); }",
        );
        assert_eq!(recovery.status, "parse_recovery_excluded");
        assert!(recovery.nodes.is_empty());
        assert!(recovery.edges.is_empty());
    }

    #[test]
    fn property_access_never_becomes_a_flow_source_or_assignment_target() {
        let source = "function run(input, object) { let output = object.value; object.value = input; return output; }";
        let report = extract_source(
            SourceLanguage::JavaScript,
            "src/parser-facts-v1/property-flow-negative.js",
            source,
        );
        let property_ids = report
            .nodes
            .iter()
            .filter(|node| node.node_kind == MicroNodeKind::PropertyAccess)
            .map(|node| node.micro_node_id.as_str())
            .collect::<BTreeSet<_>>();
        let output = report
            .nodes
            .iter()
            .find(|node| {
                node.node_kind == MicroNodeKind::LocalBinding && node.name_or_literal == "output"
            })
            .expect("output binding");
        let object = report
            .nodes
            .iter()
            .find(|node| {
                node.node_kind == MicroNodeKind::Parameter && node.name_or_literal == "object"
            })
            .expect("object binding");
        assert!(
            report.edges.iter().all(|edge| {
                edge.micro_edge_kind != MicroEdgeKind::LocalFlowsTo
                    || (!property_ids.contains(edge.head_micro_node_id.as_str())
                        && !property_ids.contains(edge.tail_micro_node_id.as_str())
                        && edge.tail_micro_node_id != output.micro_node_id)
            }),
            "{report:#?}"
        );
        assert!(
            report.edges.iter().all(|edge| {
                !matches!(
                    edge.micro_edge_kind,
                    MicroEdgeKind::LocalWrites | MicroEdgeKind::LocalMutates
                ) || (!property_ids.contains(edge.tail_micro_node_id.as_str())
                    && edge.tail_micro_node_id != object.micro_node_id)
            }),
            "{report:#?}"
        );
    }

    #[test]
    fn node_inventory_is_deterministic_and_roles_never_activate_production() {
        for (prefix, expected_role) in [
            ("src/parser-facts-v1", MicroSourceRole::Production),
            ("tests/parser-facts-v1", MicroSourceRole::Test),
            ("src/generated", MicroSourceRole::Generated),
        ] {
            let first = extract(SourceLanguage::Python, prefix);
            let second = extract(SourceLanguage::Python, prefix);
            assert_eq!(first, second, "{prefix}");
            assert_eq!(first.source_role, expected_role, "{prefix}");
            assert!(!first.production_activation_enabled);
            assert!(first
                .nodes
                .iter()
                .all(|node| node.claimability == PARSER_FACTS_V1_CLAIMABILITY
                    && node.source_role == expected_role));
            assert!(first.edges.iter().all(|edge| {
                edge.claimability == PARSER_FACTS_V1_CLAIMABILITY
                    && edge.source_role == expected_role
            }));
        }
    }

    #[test]
    fn syntax_recovery_is_a_gap_and_emits_no_semantic_nodes() {
        let path = "src/parser-facts-v1/broken.py";
        let source = "def run(input):\n    if input\n        return input\n";
        let parsed = TreeSitterParser
            .parse_source(path, source, SourceLanguage::Python)
            .expect("tree-sitter recovery parse");
        let bundle = extract_parser_fact_bundle(&parsed, source);
        let report = ParserFactsV1::extract(&parsed, source, &bundle);
        assert_eq!(report.status, "parse_recovery_excluded");
        assert!(report.nodes.is_empty());
        assert!(report.edges.is_empty());
        assert!(report
            .gaps
            .iter()
            .any(|gap| gap.gap_kind == "parse_recovery"));
    }
}
