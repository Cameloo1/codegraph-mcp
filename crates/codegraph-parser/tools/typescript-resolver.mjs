#!/usr/bin/env node
import { createRequire } from "node:module";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const [operation, repoRootRaw, repoRelativePath, query] = process.argv.slice(2);
const repoRoot = resolve(repoRootRaw || ".");

function json(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function unavailable(message) {
  json({
    status: "unavailable",
    message,
    capabilities: capabilities(false),
    resolutions: [],
  });
}

function capabilities(compilerAvailable) {
  return {
    compiler_resolver_available: compilerAvailable,
    lsp_resolver_available: false,
    supported_languages: ["typescript", "tsx"],
    exactness_when_available: "compiler_verified",
    known_limitations: [
      "Requires a resolvable TypeScript package",
      "Used only as an optional verifier; parser-only indexing still works",
    ],
  };
}

function loadTypeScript() {
  const candidates = [
    join(repoRoot, "package.json"),
    join(process.cwd(), "package.json"),
    import.meta.url,
  ];
  for (const candidate of candidates) {
    try {
      const require = createRequire(candidate);
      return require("typescript");
    } catch {
      // Try the next resolution root.
    }
  }
  return null;
}

const ts = loadTypeScript();
if (!ts) {
  unavailable("TypeScript package is not available to the optional resolver");
  process.exit(0);
}

const targetFile = resolve(repoRoot, repoRelativePath || "");
const configPath = ts.findConfigFile(repoRoot, ts.sys.fileExists, "tsconfig.json");
let fileNames = [targetFile];
let options = {
  allowJs: true,
  checkJs: true,
  moduleResolution: ts.ModuleResolutionKind.NodeJs,
  target: ts.ScriptTarget.Latest,
};

if (configPath && existsSync(configPath)) {
  const config = ts.readConfigFile(configPath, ts.sys.readFile);
  if (!config.error) {
    const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, dirname(configPath));
    fileNames = parsed.fileNames.length ? parsed.fileNames : fileNames;
    options = { ...parsed.options, allowJs: true, checkJs: true };
  }
}

if (!fileNames.includes(targetFile)) {
  fileNames.push(targetFile);
}

const program = ts.createProgram(fileNames, options);
const checker = program.getTypeChecker();
const sourceFile = program.getSourceFile(targetFile);

if (!sourceFile) {
  json({
    status: "ok",
    capabilities: capabilities(true),
    resolutions: [],
    message: `Source file not found in TypeScript program: ${repoRelativePath}`,
  });
  process.exit(0);
}

function spanFor(node) {
  const start = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile));
  const end = sourceFile.getLineAndCharacterOfPosition(node.getEnd());
  return {
    repo_relative_path: repoRelativePath.replaceAll("\\\\", "/"),
    start_line: start.line + 1,
    start_column: start.character + 1,
    end_line: end.line + 1,
    end_column: end.character + 1,
  };
}

function symbolName(symbol) {
  if (!symbol) return "";
  if (symbol.flags & ts.SymbolFlags.Alias) {
    try {
      return checker.getAliasedSymbol(symbol).getName();
    } catch {
      return symbol.getName();
    }
  }
  return symbol.getName();
}

function resolution(node, symbol, metadata = {}) {
  const resolved = symbolName(symbol) || node.getText(sourceFile);
  return {
    query,
    resolved_name: resolved,
    repo_relative_path: repoRelativePath.replaceAll("\\\\", "/"),
    source_span: spanFor(node),
    exactness: "compiler_verified",
    confidence: 1.0,
    metadata: {
      operation,
      node_kind: ts.SyntaxKind[node.kind],
      ...metadata,
    },
  };
}

const resolutions = [];

function visit(node) {
  if (!query) return;

  if (ts.isIdentifier(node) && node.text === query) {
    const symbol = checker.getSymbolAtLocation(node);
    if (symbol) {
      resolutions.push(resolution(node, symbol));
    }
  }

  if (
    operation === "resolve_import" &&
    ts.isStringLiteralLike(node) &&
    node.text.includes(query)
  ) {
    resolutions.push(resolution(node, checker.getSymbolAtLocation(node), { import_text: node.text }));
  }

  if (
    operation === "resolve_call_target" &&
    ts.isCallExpression(node) &&
    node.expression.getText(sourceFile).includes(query)
  ) {
    resolutions.push(resolution(node.expression, checker.getSymbolAtLocation(node.expression)));
  }

  if (operation === "resolve_type" && ts.isIdentifier(node) && node.text === query) {
    const type = checker.getTypeAtLocation(node);
    resolutions.push(resolution(node, checker.getSymbolAtLocation(node), {
      type: checker.typeToString(type),
    }));
  }

  ts.forEachChild(node, visit);
}

visit(sourceFile);

json({
  status: "ok",
  capabilities: capabilities(true),
  resolutions,
});
