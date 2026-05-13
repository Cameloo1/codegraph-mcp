(function () {
  "use strict";

  const relations = [
    "CALLS",
    "READS",
    "WRITES",
    "FLOWS_TO",
    "MUTATES",
    "EXPOSES",
    "AUTHORIZES",
    "CHECKS_ROLE",
    "CHECKS_PERMISSION",
    "SANITIZES",
    "VALIDATES",
    "PUBLISHES",
    "EMITS",
    "CONSUMES",
    "LISTENS_TO",
    "SPAWNS",
    "AWAITS",
    "MIGRATES",
    "READS_TABLE",
    "WRITES_TABLE",
    "ALTERS_COLUMN",
    "TESTS",
    "ASSERTS",
    "MOCKS",
    "STUBS",
    "COVERS",
  ];

  const state = {
    graph: { nodes: [], edges: [] },
    lastPayload: null,
    lastContextPacket: null,
    selectedRelations: new Set(),
    selectedSeed: "",
  };

  const byId = (id) => document.getElementById(id);

  function encode(params) {
    const search = new URLSearchParams();
    Object.entries(params).forEach(([key, value]) => {
      if (value) search.set(key, value);
    });
    return search.toString();
  }

  async function getJson(path, params) {
    const query = params ? `?${encode(params)}` : "";
    const response = await fetch(`${path}${query}`);
    const payload = await response
      .json()
      .catch(() => ({ status: "error", message: "server returned a non-JSON response" }));
    if (!response.ok || payload.status === "error") {
      throw new Error(payload.message || payload.error || "request failed");
    }
    return payload;
  }

  function shortText(value, limit) {
    const text = String(value || "");
    if (text.length <= limit) return text;
    return `${text.slice(0, Math.max(0, limit - 3))}...`;
  }

  function renderGraphSummary(payload) {
    const graph = payload.graph || { nodes: [], edges: [], relation_counts: {} };
    state.lastPayload = payload;
    const relationSummary = Object.entries(graph.relation_counts || {})
      .map(([relation, count]) => `${relation}:${count}`)
      .join(" ");
    byId("graph-summary").textContent =
      `${graph.nodes.length} nodes, ${graph.edges.length} edges` +
      (relationSummary ? ` - ${relationSummary}` : "");
    const guardrails = graph.guardrails || {};
    const warning = byId("truncation-warning");
    if (guardrails.truncated) {
      warning.hidden = false;
      warning.textContent = guardrails.truncation_warning || "Large graph truncated.";
    } else {
      warning.hidden = true;
      warning.textContent = "";
    }
    byId("edge-detail").textContent = JSON.stringify(
      {
        proof: payload.proof,
        mode: payload.filters && payload.filters.mode,
        paths: (payload.paths || []).length,
        filters: payload.filters,
        layout: graph.layout,
        guardrails: graph.guardrails,
        examples: payload.examples,
      },
      null,
      2,
    );
  }

  function renderFilters() {
    const root = byId("relation-filters");
    root.innerHTML = "";
    relations.forEach((relation) => {
      const label = document.createElement("label");
      label.className = "filter";
      const input = document.createElement("input");
      input.type = "checkbox";
      input.value = relation;
      input.addEventListener("change", () => {
        if (input.checked) state.selectedRelations.add(relation);
        else state.selectedRelations.delete(relation);
        loadGraph();
      });
      label.appendChild(input);
      label.appendChild(document.createTextNode(relation));
      root.appendChild(label);
    });
  }

  function renderLegend(style) {
    const root = byId("exactness-legend");
    root.innerHTML = "";
    Object.entries((style && style.exactness) || {}).forEach(([name, item]) => {
      const row = document.createElement("div");
      row.className = "legend-item";
      const swatch = document.createElement("span");
      swatch.className = `legend-swatch ${item.line || ""}`;
      swatch.style.borderColor = item.stroke || "#70b7ff";
      const label = document.createElement("span");
      label.textContent = name;
      row.appendChild(swatch);
      row.appendChild(label);
      root.appendChild(row);
    });
  }

  function selectedRelationText() {
    return Array.from(state.selectedRelations).join(",");
  }

  async function loadStatus() {
    const status = await getJson("/api/status");
    byId("status-line").textContent = `${status.files} files, ${status.entities} entities, ${status.edges} edges`;
  }

  async function loadGraph() {
    const source = byId("source-input").value.trim();
    const target = byId("target-input").value.trim();
    const mode = byId("mode-select").value;
    const payload = await getJson("/api/path-graph", {
      source,
      target,
      mode,
      relations: selectedRelationText(),
    });
    state.graph = payload.graph;
    state.selectedSeed = payload.resolved_source || payload.source || "";
    renderGraphSummary(payload);
    renderLegend(payload.graph && payload.graph.style);
    renderGraph(payload.graph);
    byId("source-span").textContent = JSON.stringify(payload.source_spans || [], null, 2);
  }

  function renderGraph(graph) {
    if (!window.d3) {
      byId("graph-summary").textContent = "D3 asset failed to load.";
      return;
    }

    const host = byId("graph");
    host.innerHTML = "";
    const width = Math.max(640, host.clientWidth || 640);
    const height = Math.max(420, host.clientHeight || 420);
    const svg = d3
      .select(host)
      .append("svg")
      .attr("viewBox", `0 0 ${width} ${height}`)
      .attr("role", "img")
      .attr("aria-label", "CodeGraph proof path graph");

    const nodes = graph.nodes || [];
    const edges = graph.edges || [];
    const levels = layeredPositions(nodes, edges);
    const maxLevel = Math.max(0, ...Array.from(levels.values()).map((item) => item.level));
    const rowsByLevel = new Map();
    levels.forEach((item) => {
      rowsByLevel.set(item.level, Math.max(rowsByLevel.get(item.level) || 0, item.row + 1));
    });
    const positioned = new Map();
    nodes.forEach((node) => {
      const item = levels.get(node.id) || { level: 0, row: 0 };
      const rows = rowsByLevel.get(item.level) || 1;
      positioned.set(node.id, {
        ...node,
        x: (item.level + 1) * (width / (maxLevel + 2)),
        y: (item.row + 1) * (height / (rows + 1)),
      });
    });

    const defs = svg.append("defs");
    defs
      .append("marker")
      .attr("id", "arrowhead")
      .attr("viewBox", "0 -5 10 10")
      .attr("refX", 38)
      .attr("refY", 0)
      .attr("markerWidth", 7)
      .attr("markerHeight", 7)
      .attr("orient", "auto")
      .append("path")
      .attr("d", "M0,-5L10,0L0,5")
      .attr("class", "arrowhead");

    edges.forEach((edge) => {
      const source = positioned.get(edge.source);
      const target = positioned.get(edge.target);
      if (!source || !target) return;
      svg
        .append("line")
        .attr("x1", source.x)
        .attr("y1", source.y)
        .attr("x2", target.x)
        .attr("y2", target.y)
        .attr("class", `edge ${edge.exactness || ""}`)
        .attr("stroke-width", Math.max(1.5, 1.5 + Number(edge.confidence || 0)))
        .attr("opacity", Math.max(0.4, Number(edge.confidence || 0.5)))
        .attr("marker-end", "url(#arrowhead)")
        .on("click", () => selectEdge(edge));
      const midX = (source.x + target.x) / 2;
      const midY = (source.y + target.y) / 2;
      svg
        .append("text")
        .attr("x", midX)
        .attr("y", midY - 8)
        .attr("class", "edge-label")
        .text(shortText(edge.relation, 18));
    });

    positioned.forEach((node) => {
      const group = svg.append("g").attr("class", "node");
      group.append("title").text(`${node.label || node.id}\n${node.kind || "entity"}`);
      group
        .append("circle")
        .attr("cx", node.x)
        .attr("cy", node.y)
        .attr("r", 28)
        .on("click", () => selectNode(node));
      group
        .append("text")
        .attr("x", node.x)
        .attr("y", node.y + 46)
        .attr("text-anchor", "middle")
        .text(shortText(node.label || node.id, 28));
      group
        .append("text")
        .attr("x", node.x)
        .attr("y", node.y + 62)
        .attr("text-anchor", "middle")
        .attr("class", "node-kind")
        .text(shortText(node.kind || "entity", 20));
    });

    if (nodes.length === 0) {
      d3.select(host)
        .append("div")
        .attr("class", "empty-state")
        .text("No verified paths match the current query.");
    }
  }

  function layeredPositions(nodes, edges) {
    const incoming = new Map(nodes.map((node) => [node.id, []]));
    const outgoing = new Map(nodes.map((node) => [node.id, []]));
    edges.forEach((edge) => {
      if (!incoming.has(edge.target) || !outgoing.has(edge.source)) return;
      incoming.get(edge.target).push(edge.source);
      outgoing.get(edge.source).push(edge.target);
    });
    const level = new Map(nodes.map((node) => [node.id, 0]));
    for (let pass = 0; pass < Math.min(nodes.length, 24); pass += 1) {
      edges.forEach((edge) => {
        if (!level.has(edge.source) || !level.has(edge.target)) return;
        level.set(edge.target, Math.max(level.get(edge.target), level.get(edge.source) + 1));
      });
    }
    const byLevel = new Map();
    nodes.forEach((node) => {
      const value = Math.min(level.get(node.id) || 0, Math.max(0, nodes.length - 1));
      const bucket = byLevel.get(value) || [];
      bucket.push(node.id);
      byLevel.set(value, bucket);
    });
    const positions = new Map();
    byLevel.forEach((ids, levelValue) => {
      ids.sort().forEach((id, row) => positions.set(id, { level: levelValue, row }));
    });
    return positions;
  }

  async function selectEdge(edge) {
    byId("edge-detail").textContent = JSON.stringify(edge, null, 2);
    byId("source-span").textContent = JSON.stringify(edge.source_span || {}, null, 2);
    const span = edge.source_span || {};
    if (span.repo_relative_path) {
      try {
        const payload = await getJson("/api/source-span", {
          file: span.repo_relative_path,
          start: span.start_line,
          end: span.end_line,
        });
        byId("source-span").textContent = JSON.stringify(payload, null, 2);
      } catch (error) {
        byId("source-span").textContent = error.message;
      }
    }
  }

  async function selectNode(node) {
    byId("edge-detail").textContent = JSON.stringify(node, null, 2);
    const span = node.source_span || {};
    if (span.repo_relative_path) {
      const payload = await getJson("/api/source-span", {
        file: span.repo_relative_path,
        start: span.start_line,
        end: span.end_line,
      });
      byId("source-span").textContent = JSON.stringify(payload, null, 2);
    }
    byId("source-input").value = node.label || node.id;
  }

  async function searchSymbols() {
    const query = byId("symbol-search-input").value.trim();
    const root = byId("symbol-results");
    root.innerHTML = "";
    if (query.length < 2) return;
    const payload = await getJson("/api/symbol-search", { query, limit: "8" });
    (payload.hits || []).forEach((hit) => {
      const entity = hit.entity || hit;
      const button = document.createElement("button");
      button.className = "symbol-hit";
      button.type = "button";
      button.textContent = `${entity.qualified_name || entity.name || entity.id} · ${entity.kind || ""}`;
      button.addEventListener("click", () => {
        byId("source-input").value = entity.qualified_name || entity.name || entity.id;
        loadGraph().catch(showError);
      });
      root.appendChild(button);
    });
  }

  async function loadImpact() {
    const target = byId("impact-input").value.trim() || byId("source-input").value.trim();
    if (!target) return;
    const payload = await getJson("/api/impact", { target });
    const root = byId("impact-dashboard");
    root.innerHTML = "";
    const entries = Object.entries(payload.summary || {});
    if (entries.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty-state";
      empty.textContent = "No impact paths found for this target.";
      root.appendChild(empty);
      return;
    }
    entries.forEach(([name, count]) => {
      const card = document.createElement("div");
      card.className = "metric";
      const value = document.createElement("strong");
      value.textContent = String(count);
      const label = document.createElement("span");
      label.textContent = name.replaceAll("_", " ");
      card.appendChild(value);
      card.appendChild(label);
      root.appendChild(card);
    });
  }

  async function loadContext() {
    const task = byId("task-input").value.trim() || "Inspect proof paths";
    const seed = byId("source-input").value.trim() || byId("impact-input").value.trim() || state.selectedSeed;
    const payload = await getJson("/api/context-pack", {
      task,
      seed,
      mode: "impact",
      budget: "1200",
    });
    state.lastContextPacket = payload.packet;
    byId("context-preview").textContent = JSON.stringify(payload.packet, null, 2);
  }

  function wireEvents() {
    byId("query-form").addEventListener("submit", (event) => {
      event.preventDefault();
      loadGraph().catch(showError);
    });
    byId("impact-button").addEventListener("click", () => loadImpact().catch(showError));
    byId("context-button").addEventListener("click", () => loadContext().catch(showError));
    byId("copy-context-button").addEventListener("click", () => {
      const text = byId("context-preview").textContent;
      if (navigator.clipboard) navigator.clipboard.writeText(text).catch(showError);
    });
    byId("export-json-button").addEventListener("click", () => {
      const blob = new Blob([JSON.stringify(state.lastPayload || {}, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = "codegraph-proof-path.json";
      anchor.click();
      URL.revokeObjectURL(url);
    });
    byId("compare-button").addEventListener("click", async () => {
      const payload = await getJson("/api/path-compare", {
        source: byId("source-input").value.trim(),
        target: byId("target-input").value.trim(),
        compare_target: byId("impact-input").value.trim(),
        relations: selectedRelationText(),
      });
      byId("edge-detail").textContent = JSON.stringify(payload, null, 2);
    });
    byId("mode-select").addEventListener("change", () => loadGraph().catch(showError));
    byId("symbol-search-input").addEventListener("input", () => searchSymbols().catch(showError));
    document.querySelectorAll("[data-example]").forEach((button) => {
      button.addEventListener("click", () => {
        state.selectedRelations = new Set(button.dataset.example.split(","));
        document.querySelectorAll("#relation-filters input").forEach((input) => {
          input.checked = state.selectedRelations.has(input.value);
        });
        loadGraph().catch(showError);
      });
    });
  }

  function showError(error) {
    byId("graph-summary").textContent = "Request failed.";
    byId("edge-detail").textContent = error.message;
  }

  renderFilters();
  wireEvents();
  loadStatus().catch(showError);
  loadGraph().catch(showError);
})();
