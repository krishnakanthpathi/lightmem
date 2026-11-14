const $ = (id) => document.getElementById(id);
const categories = [
  "fact",
  "decision",
  "instruction",
  "preference",
  "learning",
  "goal",
  "commitment",
  "artifact",
  "event",
  "relationship",
  "observation",
  "error",
  "context",
  "password",
];
const titles = {
  overview: [
    "Your memory, at a glance.",
    "Keep your agents in sync and your next session ready.",
  ],
  memories: [
    "A home for what matters.",
    "Find useful context, inspect its source, and keep it current.",
  ],
  connections: [
    "Same project. Shared context.",
    "Give every tool an explicit connection to the right vault.",
  ],
  models: [
    "A setup that makes sense.",
    "Choose where search, answers, and extraction happen.",
  ],
  backups: [
    "Keep a copy of your context.",
    "Inspect the destination and the bundle before restoring.",
  ],
  settings: [
    "Know exactly what’s active.",
    "Your storage, configuration, and runtime boundaries.",
  ],
};
let token = sessionStorage.getItem("lightmem-workspace-key") || "";
let state,
  currentPage = "overview",
  page = 1,
  pageData,
  selectedId,
  pendingConfig,
  pendingRestore;
let querySequence = 0,
  generation = 0,
  modelDirty = false,
  configSignature = "",
  lastJobSeen,
  pollTimer,
  toastTimer;
const text = (id, value) => {
  $(id).textContent = value ?? "";
};
function node(tag, content, className) {
  const e = document.createElement(tag);
  if (content != null) e.textContent = content;
  if (className) e.className = className;
  return e;
}
function action(label, run, className = "secondary") {
  const b = node("button", label, className);
  b.type = "button";
  b.addEventListener("click", () =>
    Promise.resolve()
      .then(run)
      .catch((error) => notice(error.message, true)),
  );
  return b;
}
function pair(list, title, value) {
  list.append(node("dt", title), node("dd", value));
}
function toast(message) {
  clearTimeout(toastTimer);
  text("toast", message);
  $("toast").hidden = false;
  toastTimer = setTimeout(() => ($("toast").hidden = true), 4200);
}
function notice(message, error = false) {
  text("notice", message);
  $("notice").classList.toggle("error", error);
  $("notice").hidden = !message;
}
function readableDate(value) {
  return value ? new Date(value).toLocaleString() : "—";
}
function shortDate(value) {
  return value
    ? new Date(value).toLocaleDateString(undefined, {
        month: "short",
        day: "numeric",
      })
    : "";
}
function size(bytes) {
  return bytes >= 1048576
    ? `${(bytes / 1048576).toFixed(1)} MiB`
    : `${Math.ceil(bytes / 1024)} KiB`;
}
async function api(path, body, { blob = false } = {}) {
  const headers = { Authorization: `Bearer ${token}` };
  if (state && path !== "/v1/workspace/status")
    headers["X-LightMem-Vault"] = encodeURIComponent(state.db_path);
  if (body !== undefined) headers["Content-Type"] = "application/json";
  const response = await fetch(path, {
    method: body === undefined ? "GET" : "POST",
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
    cache: "no-store",
    credentials: "omit",
  });
  if (!response.ok) {
    let message = `Request failed (${response.status})`;
    try {
      message = (await response.json()).error || message;
    } catch {}
    if (response.status === 401) {
      $("app").hidden = true;
      $("access").hidden = false;
      sessionStorage.removeItem("lightmem-workspace-key");
    }
    throw new Error(message);
  }
  return blob ? response.blob() : response.json();
}
async function copy(value) {
  try {
    await navigator.clipboard.writeText(value);
    toast("Copied to clipboard");
  } catch {
    notice(
      "Clipboard is unavailable. Select and copy the displayed command manually.",
      true,
    );
  }
}
async function download(path, filename, body) {
  const blob = await api(path, body, { blob: true });
  const url = URL.createObjectURL(blob);
  const link = node("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60000);
}
function go(view) {
  location.hash = view;
}
function route() {
  const candidate = location.hash.slice(1);
  if (candidate === "main") {
    $("main").focus({ preventScroll: true });
    return;
  }
  currentPage = titles[candidate] ? candidate : "overview";
  document
    .querySelectorAll("[data-page]")
    .forEach((e) => (e.hidden = e.dataset.page !== currentPage));
  document
    .querySelectorAll("[data-view]")
    .forEach((e) =>
      e.dataset.view === currentPage
        ? e.setAttribute("aria-current", "page")
        : e.removeAttribute("aria-current"),
    );
  text("pageTitle", titles[currentPage][0]);
  text("pageDescription", titles[currentPage][1]);
  document.title = `${currentPage[0].toUpperCase() + currentPage.slice(1)} — LightMem`;
  if (state && currentPage === "memories")
    loadMemories().catch((e) => notice(e.message, true));
}
document
  .querySelectorAll("[data-go]")
  .forEach((e) => e.addEventListener("click", () => go(e.dataset.go)));
document
  .querySelectorAll("[data-close]")
  .forEach((e) =>
    e.addEventListener("click", () => $(e.dataset.close).close()),
  );
window.addEventListener("hashchange", route);
for (const c of categories) {
  for (const id of ["categoryFilter", "newCategory"]) {
    const option = node("option", c[0].toUpperCase() + c.slice(1));
    option.value = c;
    $(id).append(option);
  }
}

function renderContext() {
  const picker = $("vaultSelect");
  picker.replaceChildren();
  let option = node(
    "option",
    `Initial vault · ${state.initial_path.split(/[\\/]/).pop()}`,
  );
  option.value = "";
  picker.append(option);
  for (const connection of state.connections) {
    option = node(
      "option",
      `${connection.connection_id} · ${connection.project_id || connection.user_id}`,
    );
    option.value = connection.connection_id;
    picker.append(option);
  }
  picker.value = state.context?.connection_id || "";
  $("scopePicker").hidden = !state.context;
  $("scopeSelect").value = state.context?.scope || "user";
  $("scopeSelect").querySelector("[value=project]").disabled =
    !!state.context && !state.context.project_id;
  picker.disabled = state.busy;
  $("scopeSelect").disabled = state.busy;
  text("footerVault", state.db_path);
  text("versionLabel", `v${state.version} · review build`);
  text("activeCount", state.stats.active_memories);
  text("vectorCount", state.stats.total_vectors);
  text("archiveCount", state.stats.expired_memories);
  text("connectionCount", state.connections.length);
  text(
    "indexMetric",
    state.needs_reindex
      ? "Reindex required before search"
      : "Index matches the saved embedding settings",
  );
  const summary = $("setupSummary");
  summary.replaceChildren();
  pair(summary, "Selected vault", state.db_path);
  pair(summary, "Find memories", state.embedding_summary);
  pair(summary, "Produce answers", state.answer_summary);
  pair(
    summary,
    "Entity extraction",
    state.config.entity_extraction === "llm"
      ? `LLM at ${state.config.ollama_url}`
      : "Off · new writes stay local",
  );
  const steps = [
    [
      true,
      "Choose your vault",
      state.context
        ? `${state.context.connection_id} · ${state.context.scope} scope`
        : "Using the initial local vault",
      "settings",
    ],
    [
      !!state.model_test && !state.needs_reindex,
      "Prepare and test search",
      state.needs_reindex
        ? "The selected vault needs reindexing."
        : "Run a backend test before using this setup.",
      "models",
    ],
    [
      state.connections.length > 0,
      "Create a named connection",
      "Bind a tool to an explicit user/project scope.",
      "connections",
    ],
    [
      state.stats.active_memories > 0,
      "Save your first memory",
      "Add a decision, preference, or fact worth keeping.",
      "memories",
    ],
  ];
  const list = $("setupSteps");
  list.replaceChildren();
  steps.forEach(([done, title, description, view], i) => {
    const li = node("li");
    li.append(
      node(
        "span",
        done ? "✓" : String(i + 1),
        `step-number${done ? " complete" : ""}`,
      ),
    );
    const details = node("div");
    details.append(node("strong", title), node("p", description));
    li.append(
      details,
      action(done ? "View" : "Set up", () => go(view)),
    );
    list.append(li);
  });
  text("setupCount", `${steps.filter((s) => s[0]).length} / 4`);
  const details = $("settingsDetails");
  details.replaceChildren();
  for (const [label, value] of [
    ["Target", "This computer · authenticated loopback server"],
    ["Vault path", state.db_path],
    ["Connection", state.context?.connection_id || "Initial vault"],
    ["Scope", state.context?.scope || "Default database selection"],
    ["Configuration file", state.config_file],
    ["Model cache", state.model_cache],
    ["Storage protection", state.storage_protection],
    [
      "Offline environment override",
      state.environment.offline ? "Enabled" : "Not set",
    ],
    [
      "QA disabled environment override",
      state.environment.qa_disabled ? "Enabled" : "Not set",
    ],
    [
      "Saved remote endpoint",
      state.environment.remote_url_configured
        ? "Configured for CLI use; this workspace still manages local data"
        : "Not configured",
    ],
  ])
    pair(details, label, value);
  text(
    "modelReadiness",
    state.model_test
      ? `Search backend verified ${readableDate(state.model_test.at)} (${state.model_test.dimensions} dimensions). Test answers separately with a known memory.`
      : "Search backend has not been tested in this workspace session.",
  );
  text(
    "backupTime",
    state.last_backup_this_session
      ? `Backup downloaded this session: ${readableDate(state.last_backup_this_session)}`
      : "No backup downloaded in this workspace session.",
  );
  document
    .querySelectorAll("[data-write]")
    .forEach((b) => (b.disabled = state.busy));
  if (state.config_changed_externally)
    notice(
      "Configuration changed outside this workspace. Restart lmem ui before applying settings.",
    );
  else if (state.needs_reindex)
    notice(
      "This vault’s vectors use different embedding settings. Open Models & setup and reindex before searching.",
    );
  else if (state.environment.remote_url_configured)
    notice(
      "A remote CLI endpoint is configured. This workspace explicitly manages local vaults on this computer.",
    );
  else notice("");
}
function populateModels() {
  const cfg = state.config;
  $("preset").value = "custom";
  $("embeddingBackend").value = cfg.backend;
  const name = cfg.onnx_model || "bge-small";
  let model = name.toLowerCase().includes("minilm")
    ? "minilm"
    : name.toLowerCase().includes("nomic")
      ? "nomic"
      : name.toLowerCase().includes("bge-small")
        ? "bge-small"
        : name;
  if (![...$("onnxModel").options].some((o) => o.value === model)) {
    const custom = node("option", `${model} · custom (manage via CLI)`);
    custom.value = model;
    $("onnxModel").append(custom);
  }
  $("onnxModel").value = model;
  $("answerModel").value = cfg.reranker.startsWith("ollama")
    ? "ollama"
    : cfg.reranker;
  $("ollamaAnswerModel").value = cfg.reranker.startsWith("ollama:")
    ? cfg.reranker.slice(7)
    : "";
  $("entityMode").value = cfg.entity_extraction;
  $("ollamaUrl").value = cfg.ollama_url;
  $("embeddingModel").value = cfg.embedding_model;
  $("ollamaKey").value = "";
  $("clearOllamaKey").checked = false;
  text(
    "keyState",
    state.environment.ollama_key_from_environment
      ? "An environment variable supplies the Ollama key. Clearing the stored key will not remove it."
      : cfg.ollama_api_key_configured
        ? "Ollama key configured; its value is hidden."
        : "No Ollama key configured.",
  );
  modelDirty = false;
  text("modelTestResult", "");
}
function renderConnections() {
  const list = $("connectionsList");
  list.replaceChildren();
  text("connectionsTotal", `${state.connections.length} named connections`);
  if (!state.connections.length)
    list.append(
      empty(
        "Your tools can share a little memory.",
        "Create a named connection to make vault selection explicit.",
        "Create connection",
        () => $("connectionDialog").showModal(),
      ),
    );
  for (const c of state.connections) {
    const row = node("div", null, "connection-row");
    const info = node("div");
    info.append(
      node("strong", c.connection_id),
      node(
        "p",
        `${c.harness_id} · ${c.user_id} / ${c.agent_id} · ${c.project_id || "No project"} · ${c.scope}`,
      ),
      node("p", c.db_path, "mono quiet"),
    );
    const root = state.config_file.replace(/[\\/][^\\/]+$/, "");
    const shellQuote = (v) => `'${String(v).replaceAll("'", "'\"'\"'")}'`;
    const psQuote = (v) => `'${String(v).replaceAll("'", "''")}'`;
    const prefix =
      state.os === "windows"
        ? `$env:LIGHTMEM_CONFIG_DIR=${psQuote(root)}; & ${psQuote(state.binary_path || "lmem")}`
        : `LIGHTMEM_CONFIG_DIR=${shellQuote(root)} ${shellQuote(state.binary_path || "lmem")}`;
    const command = `${prefix} --connection ${c.connection_id} --scope ${c.scope} context --json`;
    const code = node("code", command);
    info.append(code);
    const buttons = node("div", null, "actions");
    buttons.append(
      action("Use vault", () => selectVault(c.connection_id, c.scope)),
      action("Copy test command", () => copy(command)),
    );
    const platform = state.platforms.find((p) => p.id === c.harness_id);
    buttons.append(
      action("Install skill", () =>
        confirmAction(
          "Install this connection’s agent skill?",
          `Tool: ${c.harness_id}. Destination: ${c.skill_path || platform?.target_file || "the tool’s configured skill path"}. An existing LightMem binding at this destination will be replaced.`,
          async () => {
            await api("/v1/workspace/connections/install", {
              id: c.connection_id,
              mcp: false,
              confirm: true,
            });
            toast(
              "Binding installed. Reload your tool, then test a memory cycle.",
            );
          },
        ),
      ),
    );
    if (["claude", "cursor", "antigravity"].includes(c.harness_id))
      buttons.append(
        action("Configure MCP", () =>
          confirmAction(
            "Configure MCP for this tool?",
            `This updates ${c.harness_id}’s MCP configuration to use connection ${c.connection_id}. Reload the tool after applying.`,
            async () => {
              await api("/v1/workspace/connections/install", {
                id: c.connection_id,
                mcp: true,
                confirm: true,
              });
              toast("MCP binding configured. Reload your tool to activate it.");
            },
          ),
        ),
      );
    row.append(info, buttons);
    buttons
      .querySelectorAll("button")
      .forEach(
        (b) =>
          (b.disabled = state.busy && b.textContent !== "Copy test command"),
      );
    list.append(row);
  }
  const tools = $("platformList");
  tools.replaceChildren();
  for (const p of state.platforms) {
    const card = node("div", null, "tool");
    card.append(
      node("strong", p.name),
      node("p", p.detected ? "Application detected" : "Manual setup available"),
      node("code", p.target_file),
    );
    tools.append(card);
  }
}
function renderJob() {
  clearTimeout(pollTimer);
  const job = state.job;
  $("jobPanel").hidden = !job;
  if (!job) return;
  text(
    "jobTitle",
    `${job.kind[0].toUpperCase() + job.kind.slice(1)} · ${job.status}`,
  );
  text("jobMessage", job.message);
  $("jobPanel").classList.toggle("failed", job.status === "failed");
  $("jobProgress").hidden = job.status !== "running";
  if (job.total) {
    $("jobProgress").max = job.total;
    $("jobProgress").value = job.done;
  } else $("jobProgress").removeAttribute("value");
  if (job.status === "running")
    pollTimer = setTimeout(
      () =>
        refreshStatus().catch((e) =>
          notice(
            `Operation status unavailable: ${e.message}. Use Refresh to retry.`,
            true,
          ),
        ),
      1500,
    );
  else if (lastJobSeen !== `${job.id}:${job.status}`) {
    lastJobSeen = `${job.id}:${job.status}`;
    if (job.status === "complete") {
      modelDirty = false;
      populateModels();
      loadRecent().catch(() => {});
      if (currentPage === "memories")
        loadMemories().catch((e) => notice(e.message, true));
      toast(job.message);
    }
  }
}
async function refreshStatus() {
  const next = await api("/v1/workspace/status");
  if (state && state.db_path !== next.db_path) {
    generation++;
    selectedId = null;
    pendingRestore = null;
    $("restorePanel").hidden = true;
  }
  state = next;
  renderContext();
  renderConnections();
  renderJob();
  const signature = JSON.stringify(state.config);
  if (!configSignature || (signature !== configSignature && !modelDirty)) {
    populateModels();
    configSignature = signature;
  }
  $("app").hidden = false;
  $("access").hidden = true;
}
async function connect() {
  await refreshStatus();
  sessionStorage.setItem("lightmem-workspace-key", token);
  route();
  await loadRecent();
}
$("accessForm").addEventListener("submit", async (e) => {
  e.preventDefault();
  token = $("accessKey").value.trim();
  text("accessError", "");
  try {
    await connect();
    $("accessKey").value = "";
  } catch (error) {
    text("accessError", error.message);
  }
});
$("lockWorkspace").addEventListener("click", () => {
  clearTimeout(pollTimer);
  token = "";
  sessionStorage.removeItem("lightmem-workspace-key");
  $("app").hidden = true;
  $("access").hidden = false;
  $("accessKey").focus();
});
$("refresh").addEventListener("click", async () => {
  try {
    await refreshStatus();
    await loadRecent();
    if (currentPage === "memories") await loadMemories();
    toast("Workspace refreshed");
  } catch (error) {
    notice(error.message, true);
  }
});
async function selectVault(connection, scope) {
  await api(
    "/v1/workspace/vault",
    connection ? { connection, scope } : { connection: null },
  );
  generation++;
  selectedId = null;
  pendingRestore = null;
  $("restorePanel").hidden = true;
  page = 1;
  await refreshStatus();
  await loadRecent();
  if (currentPage === "memories") await loadMemories();
  toast("Active vault changed");
}
$("vaultSelect").addEventListener("change", () =>
  selectVault($("vaultSelect").value).catch((e) => {
    notice(e.message, true);
    renderContext();
  }),
);
$("scopeSelect").addEventListener("change", () =>
  selectVault($("vaultSelect").value, $("scopeSelect").value).catch((e) => {
    notice(e.message, true);
    renderContext();
  }),
);
function empty(title, description, label, run) {
  const box = node("div", null, "empty");
  box.append(
    node("span", "▤", "empty-icon"),
    node("h2", title),
    node("p", description),
  );
  if (label) box.append(action(label, run, "primary"));
  return box;
}
function row(memory, score) {
  const b = action(
    "",
    () => {
      selectedId = memory.id;
      go("memories");
      showDetail(memory.id).catch((e) => notice(e.message, true));
    },
    "memory-row",
  );
  b.dataset.memoryId = memory.id;
  b.classList.toggle("selected", memory.id === selectedId);
  if (memory.id === selectedId) b.setAttribute("aria-current", "true");
  const meta = node("div", null, "row-meta");
  meta.append(
    node("span", memory.category, "row-category"),
    node("span", shortDate(memory.updated_at)),
  );
  b.append(
    meta,
    node("h3", memory.title || "Untitled memory"),
    node(
      "p",
      memory.content.slice(0, 170) + (memory.content.length > 170 ? "…" : ""),
    ),
  );
  if (memory.tags?.length)
    b.append(
      node(
        "div",
        memory.tags
          .slice(0, 4)
          .map((t) => `#${t}`)
          .join("  "),
        "row-tags",
      ),
    );
  if (score != null)
    b.append(node("div", `Ranking score ${score.toFixed(3)}`, "quiet"));
  return b;
}
async function loadRecent() {
  const epoch = generation;
  const data = await api("/v1/list?limit=4&status=active");
  if (epoch !== generation) return;
  const list = $("recentMemories");
  list.replaceChildren();
  if (!data.items.length)
    list.append(
      empty(
        "Your next session starts here.",
        "Save a decision or preference worth keeping.",
        "Add your first memory",
        openMemory,
      ),
    );
  else data.items.forEach((m) => list.append(row(m)));
}
function queryOptions() {
  const q = {};
  if ($("categoryFilter").value) q.category = $("categoryFilter").value;
  if ($("dateFilter").value) q.date = $("dateFilter").value;
  return q;
}
async function loadMemories() {
  const sequence = ++querySequence,
    epoch = generation;
  const list = $("memoryList");
  list.replaceChildren(node("p", "Loading memories…", "empty quiet"));
  $("answerCard").hidden = true;
  const query = $("memoryQuery").value.trim(),
    mode = $("queryMode").value;
  try {
    let items = [];
    let scores = new Map();
    let listing = null;
    if (query && mode === "answer") {
      if ($("mediaFilter").value || $("statusFilter").value !== "active")
        throw new Error(
          "Answers use active text memories. Clear the media filter and select Active.",
        );
      const result = await api("/v1/answer", {
        question: query,
        limit: 10,
        ...queryOptions(),
      });
      if (sequence !== querySequence || epoch !== generation) return;
      text("answerText", result.answer);
      text(
        "answerEngine",
        `Answer engine: ${result.reranker_used}${result.confidence === 0 ? " · No supporting evidence found" : ""}`,
      );
      $("answerCard").hidden = false;
      $("answerSource").hidden = !result.selected_memory;
      $("answerSource").onclick = () =>
        result.selected_memory &&
        showDetail(result.selected_memory.id).catch((e) =>
          notice(e.message, true),
        );
      if (result.selected_memory) items = [result.selected_memory];
    } else if (query) {
      if ($("statusFilter").value !== "active")
        throw new Error(
          "Clear the search to browse archived memories. Search retrieves active memories.",
        );
      const results = await api("/v1/recall", {
        query,
        limit: 20,
        ...queryOptions(),
        ...($("mediaFilter").value ? { media: $("mediaFilter").value } : {}),
      });
      items = results.map((r) => r.memory);
      scores = new Map(results.map((r) => [r.memory.id, r.score]));
    } else {
      const params = new URLSearchParams({
        limit: "20",
        page: String(page),
        status: $("statusFilter").value,
        ...queryOptions(),
      });
      if ($("mediaFilter").value) params.set("media", $("mediaFilter").value);
      listing = await api(`/v1/list?${params}`);
      items = listing.items;
    }
    if (sequence !== querySequence || epoch !== generation) return;
    pageData = listing;
    list.replaceChildren();
    text(
      "resultCount",
      pageData
        ? `${pageData.total} total`
        : `${items.length} result${items.length === 1 ? "" : "s"}`,
    );
    if (!items.length)
      list.append(
        empty(
          "No memories here yet.",
          query
            ? "Try another phrase or clear your filters."
            : "Add a fact, decision, or file to this vault.",
          "Add memory",
          openMemory,
        ),
      );
    else items.forEach((m) => list.append(row(m, scores.get(m.id))));
    $("pagination").hidden = !pageData || pageData.total_pages <= 1;
    if (pageData) {
      text("pageNumber", `${pageData.page} / ${pageData.total_pages || 1}`);
      $("previousPage").disabled = page <= 1;
      $("nextPage").disabled = !pageData.has_more;
    }
    if (selectedId && items.some((m) => m.id === selectedId))
      await showDetail(selectedId);
    else {
      selectedId = null;
      $("memoryDetail").replaceChildren(
        empty(
          "A little context goes a long way.",
          "Select a memory to inspect its content, source, files, and relationships.",
        ),
      );
    }
  } catch (error) {
    if (sequence !== querySequence) return;
    list.replaceChildren(
      empty(
        "Couldn’t load these memories.",
        error.message,
        "Retry",
        loadMemories,
      ),
    );
    $("pagination").hidden = true;
    text("resultCount", "");
    throw error;
  }
}
async function showDetail(id) {
  selectedId = id;
  const epoch = generation;
  const [memory, links] = await Promise.all([
    api(`/v1/get?term=${encodeURIComponent(id)}`),
    api(`/v1/links?id=${encodeURIComponent(id)}`),
  ]);
  if (selectedId !== id || epoch !== generation) return;
  document.querySelectorAll(".memory-row").forEach((b) => {
    b.classList.toggle("selected", b.dataset.memoryId === id);
    if (b.dataset.memoryId === id) b.setAttribute("aria-current", "true");
    else b.removeAttribute("aria-current");
  });
  const detail = $("memoryDetail");
  detail.replaceChildren(
    node(
      "span",
      `${memory.category} · ${memory.status === "active" ? "Active" : "Archived"}`,
      "badge",
    ),
    node("h2", memory.title, "detail-title"),
    node("div", memory.content, "memory-content"),
  );
  if (memory.tags?.length) {
    const tags = node("div", null, "tag-list");
    memory.tags.forEach((t) => tags.append(node("span", `#${t}`, "tag")));
    detail.append(tags);
  }
  const meta = node("dl", null, "detail-section");
  pair(meta, "Updated", readableDate(memory.updated_at));
  pair(meta, "Created", readableDate(memory.created_at));
  pair(meta, "Source", memory.provenance);
  pair(
    meta,
    "Connection / scope",
    memory.context
      ? `${memory.context.connection_id} · ${memory.context.scope}`
      : "No named connection attribution",
  );
  if (memory.context?.session_id)
    pair(meta, "Session", memory.context.session_id);
  if (memory.expired_at)
    pair(
      meta,
      memory.status === "active" ? "Expires" : "Expired",
      readableDate(memory.expired_at),
    );
  detail.append(meta);
  if (memory.attachments?.length) {
    const section = node("div", null, "detail-section");
    section.append(node("h3", "ATTACHMENTS"));
    for (const file of memory.attachments) {
      const item = node("div", null, "attachment");
      const title = node("div", file.filename);
      title.append(
        node("small", `${file.mime_type} · ${size(file.size_bytes)}`),
      );
      item.append(
        title,
        action("Download", () =>
          download(
            `/v1/attachment?memory=${encodeURIComponent(memory.id)}&id=${encodeURIComponent(file.id)}`,
            file.filename,
          ),
        ),
      );
      section.append(item);
    }
    detail.append(section);
  }
  if (memory.entities?.length) {
    const entities = node("dl", null, "detail-section");
    entities.append(node("h3", "EXTRACTED ENTITIES"));
    memory.entities.forEach((e) => pair(entities, e.name, e.val));
    detail.append(entities);
  }
  const relationships = node("div", null, "detail-section");
  relationships.append(node("h3", `RELATIONSHIPS · ${links.length}`));
  if (!links.length)
    relationships.append(node("p", "No direct relationships.", "quiet"));
  else
    for (const link of links) {
      const other =
        link.source_id === memory.id ? link.target_id : link.source_id;
      relationships.append(
        action(
          `${link.relation} → ${other.slice(0, 8)}`,
          () => showDetail(other),
          "text-button",
        ),
      );
    }
  detail.append(relationships);
  const actions = node("div", null, "detail-actions");
  actions.append(action("Copy memory ID", () => copy(memory.id), "secondary"));
  if (memory.status === "active")
    actions.append(
      action(
        "Archive memory",
        () =>
          confirmAction(
            "Archive this memory?",
            `“${memory.title}” will be excluded from current recall. Its record and files remain stored. Relationships are hidden while a memory is archived.`,
            async () => {
              await api("/v1/forget", { id: memory.id, hard: false });
              selectedId = null;
              await refreshStatus();
              await loadMemories();
              await loadRecent();
              toast("Memory archived");
            },
          ),
        "danger-button",
      ),
    );
  else
    actions.append(
      action(
        "Restore memory",
        () =>
          confirmAction(
            "Restore this memory?",
            `“${memory.title}” will become active again without its previous expiration. Saved relationships to active memories become visible again.`,
            async () => {
              await api("/v1/workspace/memory/restore", { id: memory.id });
              $("statusFilter").value = "active";
              page = 1;
              await refreshStatus();
              await loadMemories();
              await loadRecent();
              await showDetail(memory.id);
              toast("Memory restored");
            },
          ),
        "secondary",
      ),
    );
  detail.append(actions);
  if (innerWidth < 1150)
    detail.scrollIntoView({ behavior: "instant", block: "start" });
}
$("searchForm").addEventListener("submit", (e) => {
  e.preventDefault();
  page = 1;
  loadMemories().catch((e) => notice(e.message, true));
});
for (const id of [
  "categoryFilter",
  "mediaFilter",
  "statusFilter",
  "dateFilter",
])
  $(id).addEventListener("change", () => {
    if (id === "statusFilter") $("memoryQuery").value = "";
    page = 1;
    loadMemories().catch((e) => notice(e.message, true));
  });
$("clearFilters").addEventListener("click", () => {
  $("memoryQuery").value = "";
  $("categoryFilter").value = "";
  $("mediaFilter").value = "";
  $("dateFilter").value = "";
  $("statusFilter").value = "active";
  page = 1;
  loadMemories().catch((e) => notice(e.message, true));
});
$("previousPage").addEventListener("click", () => {
  page--;
  loadMemories().catch((e) => notice(e.message, true));
});
$("nextPage").addEventListener("click", () => {
  page++;
  loadMemories().catch((e) => notice(e.message, true));
});
function openMemory() {
  text("memoryFormError", "");
  text(
    "writeDestination",
    `Saving to ${state.db_path}${state.context ? ` · ${state.context.connection_id} / ${state.context.scope}` : ""}`,
  );
  $("memoryDialog").showModal();
}
$("addMemory").addEventListener("click", openMemory);
async function base64File(file) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  for (let i = 0; i < bytes.length; i += 32768)
    binary += String.fromCharCode(...bytes.subarray(i, i + 32768));
  return btoa(binary);
}
$("memoryForm").addEventListener("submit", async (e) => {
  e.preventDefault();
  text("memoryFormError", "");
  const submit = e.submitter;
  submit.disabled = true;
  try {
    const files = [...$("newFiles").files];
    if (files.reduce((n, f) => n + f.size, 0) > 40 * 1048576)
      throw new Error(
        "Attachments must total 40 MiB or less. Use the CLI for larger files.",
      );
    if (!$("newContent").value.trim() && !files.length)
      throw new Error("Add memory text or attach a file.");
    const request = {
      content: $("newContent").value,
      title: $("newTitle").value.trim() || undefined,
      category: $("newCategory").value || undefined,
      tags: $("newTags")
        .value.split(",")
        .map((t) => t.trim())
        .filter(Boolean),
      confidence: Number($("newConfidence").value),
      no_entities: $("skipEntities").checked,
      ttl: $("newTtl").value.trim() || undefined,
    };
    if (files.length)
      request.attachments = await Promise.all(
        files.map(async (f) => ({
          name: f.name,
          mime: f.type || undefined,
          data_base64: await base64File(f),
        })),
      );
    const saved = await api("/v1/remember", request);
    $("memoryQuery").value = "";
    $("queryMode").value = "search";
    $("statusFilter").value = "active";
    $("categoryFilter").value = "";
    $("mediaFilter").value = "";
    $("dateFilter").value = "";
    page = 1;
    $("memoryDialog").close();
    $("memoryForm").reset();
    selectedId = saved.id;
    await refreshStatus();
    await loadRecent();
    go("memories");
    await loadMemories();
    await showDetail(saved.id);
    toast(saved.warnings?.join(" ") || "Memory saved to this vault");
  } catch (error) {
    text("memoryFormError", error.message);
  } finally {
    submit.disabled = state?.busy || false;
  }
});
$("newConnection").addEventListener("click", () =>
  $("connectionDialog").showModal(),
);
$("newScope").addEventListener("change", () => {
  $("projectId").required = $("newScope").value === "project";
});
$("projectId").required = true;
$("connectionForm").addEventListener("submit", async (e) => {
  e.preventDefault();
  text("connectionFormError", "");
  try {
    const connection = await api("/v1/workspace/connections", {
      id: $("connectionId").value,
      user: $("userId").value,
      agent: $("agentId").value,
      harness: $("harnessId").value,
      scope: $("newScope").value,
      project: $("projectId").value || null,
    });
    $("connectionDialog").close();
    $("connectionForm").reset();
    $("projectId").required = true;
    await refreshStatus();
    toast(
      `Connection ${connection.connection_id} created. Select it or install its binding.`,
    );
  } catch (error) {
    text("connectionFormError", error.message);
  }
});
let confirmation;
function confirmAction(title, message, run) {
  text("confirmTitle", title);
  text("confirmMessage", message);
  text("confirmError", "");
  confirmation = run;
  $("confirmDialog").showModal();
}
$("confirmAction").addEventListener("click", async () => {
  const b = $("confirmAction");
  b.disabled = true;
  try {
    await confirmation();
    $("confirmDialog").close();
  } catch (error) {
    text("confirmError", error.message);
  } finally {
    b.disabled = false;
  }
});
$("modelForm").addEventListener("input", () => (modelDirty = true));
$("preset").addEventListener("change", () => {
  const preset = $("preset").value;
  if (preset === "custom") return;
  $("embeddingBackend").value = preset === "demo" ? "hash" : "onnx";
  $("onnxModel").value = "bge-small";
  $("entityMode").value =
    preset === "local" || preset === "demo" ? "off" : "llm";
  $("answerModel").value =
    preset === "demo"
      ? "top1"
      : preset === "local"
        ? "minilm-squad2"
        : "ollama";
  $("ollamaAnswerModel").value =
    preset === "ollama-cloud" ? "gemma4:31b-cloud" : "";
  if (preset.startsWith("ollama"))
    $("ollamaUrl").value = "http://localhost:11434";
  modelDirty = true;
});
function configPatch() {
  const answer = $("answerModel").value;
  const patch = {
    backend: $("embeddingBackend").value,
    onnx_model: ["bge-small", "minilm", "nomic"].includes($("onnxModel").value)
      ? $("onnxModel").value
      : undefined,
    embedding_model: $("embeddingModel").value,
    ollama_url: $("ollamaUrl").value,
    reranker:
      answer === "ollama" && $("ollamaAnswerModel").value.trim()
        ? `ollama:${$("ollamaAnswerModel").value.trim()}`
        : answer,
    entity_extraction: $("entityMode").value,
    clear_ollama_key: $("clearOllamaKey").checked,
  };
  if ($("ollamaKey").value) patch.ollama_api_key = $("ollamaKey").value;
  return patch;
}
$("modelForm").addEventListener("submit", async (e) => {
  e.preventDefault();
  try {
    pendingConfig = configPatch();
    const preview = await api("/v1/workspace/config/preview", pendingConfig);
    pendingConfig.needs_reindex = preview.needs_reindex;
    const summary = $("configSummary");
    summary.replaceChildren();
    for (const [label, value] of [
      ["Embedding backend", preview.config.backend],
      ["Local model", preview.config.onnx_model],
      ["Answer engine", preview.config.reranker],
      ["Entity extraction", preview.config.entity_extraction],
      ["LLM destination", preview.config.ollama_url],
      ["Vault to migrate", preview.vault],
    ])
      pair(summary, label, value);
    text(
      "migrationDescription",
      preview.needs_reindex
        ? `${preview.records} memories need to be indexed with the new settings. The existing index remains until the replacement is complete.`
        : "No embedding migration is required.",
    );
    $("migrationConsentLabel").hidden = !preview.needs_reindex;
    $("migrationConsent").checked = false;
    text("configError", "");
    $("configDialog").showModal();
  } catch (error) {
    notice(error.message, true);
  }
});
$("applyConfig").addEventListener("click", async () => {
  text("configError", "");
  if (pendingConfig.needs_reindex && !$("migrationConsent").checked) {
    text(
      "configError",
      "Approve reindexing before applying these embedding settings.",
    );
    return;
  }
  try {
    const { needs_reindex, ...patch } = pendingConfig;
    await api("/v1/workspace/config", { ...patch, migrate: needs_reindex });
    $("configDialog").close();
    $("ollamaKey").value = "";
    modelDirty = false;
    await refreshStatus();
  } catch (error) {
    text("configError", error.message);
  }
});
async function startOperation(path, body) {
  await api(path, body);
  await refreshStatus();
}
$("downloadEmbeddings").addEventListener("click", () => {
  if ($("embeddingBackend").value !== "onnx") {
    toast("Ollama manages its models; hash mode needs no download.");
    return;
  }
  startOperation("/v1/workspace/models/download", {
    model: $("onnxModel").value,
  }).catch((e) => notice(e.message, true));
});
$("downloadAnswer").addEventListener("click", () => {
  const model = $("answerModel").value;
  if (!["minilm-squad2", "tinyroberta-squad2"].includes(model)) {
    toast("This answer engine has no ONNX model to download here.");
    return;
  }
  startOperation("/v1/workspace/models/download", { model }).catch((e) =>
    notice(e.message, true),
  );
});
$("reindex").addEventListener("click", () =>
  confirmAction(
    "Reindex the selected vault?",
    `This rebuilds embeddings for ${state.db_path} using the saved configuration. It may download a local model or contact your configured Ollama endpoint.`,
    () => startOperation("/v1/workspace/reindex", {}),
  ),
);
$("testBackend").addEventListener("click", async () => {
  $("testBackend").disabled = true;
  text("modelTestResult", "Testing the saved search backend…");
  try {
    const result = await api("/v1/workspace/test", {});
    text("modelTestResult", result.message);
    await refreshStatus();
  } catch (error) {
    text("modelTestResult", error.message);
  } finally {
    $("testBackend").disabled = state?.busy || false;
  }
});
$("exportBundle").addEventListener("click", async () => {
  try {
    await download(
      "/v1/workspace/backup",
      `lightmem-${new Date().toISOString().slice(0, 10)}.zip`,
      {},
    );
    await refreshStatus();
    toast("Backup download started. Keep it somewhere safe.");
  } catch (error) {
    notice(error.message, true);
  }
});
$("inspectBundleForm").addEventListener("submit", async (e) => {
  e.preventDefault();
  try {
    const file = $("bundleFile").files[0];
    if (file.size > 40 * 1048576)
      throw new Error("Use the CLI to inspect bundles larger than 40 MiB.");
    pendingRestore = await api("/v1/workspace/restore/preview", {
      data_base64: await base64File(file),
    });
    const summary = $("restoreSummary");
    summary.replaceChildren();
    pair(summary, "Destination", pendingRestore.destination);
    pair(summary, "Existing records", pendingRestore.current_records);
    pair(summary, "Bundle version", pendingRestore.manifest.lightmem_version);
    pair(summary, "Created", readableDate(pendingRestore.manifest.created_at));
    const stats = pendingRestore.manifest.stats;
    pair(
      summary,
      "Bundle contents",
      `${stats.total_memories} memories · ${stats.total_links} relationships · ${stats.total_attachments} files (${size(stats.total_attachment_bytes)})`,
    );
    pair(
      summary,
      "Attachment files",
      pendingRestore.manifest.attachments.length,
    );
    $("restorePanel").hidden = false;
    $("replaceConfirm").value = "";
  } catch (error) {
    pendingRestore = null;
    $("restorePanel").hidden = true;
    notice(error.message, true);
  }
});
$("restoreMode").addEventListener("change", () => {
  const replace = $("restoreMode").value === "replace";
  $("replaceConfirmLabel").hidden = !replace;
  text(
    "restoreExplanation",
    replace
      ? "Replace permanently removes this vault’s current contents and restores the bundle. Download a backup first."
      : "Merge keeps the existing vault and imports the bundle’s records according to merge rules.",
  );
});
$("restoreBundle").addEventListener("click", async () => {
  try {
    if (!pendingRestore) throw new Error("Inspect a bundle first.");
    const mode = $("restoreMode").value;
    if (mode === "replace" && $("replaceConfirm").value !== "REPLACE")
      throw new Error("Type REPLACE to confirm permanent replacement.");
    await startOperation("/v1/workspace/restore", {
      preview_id: pendingRestore.preview_id,
      mode,
      confirm_vault: pendingRestore.destination,
    });
    pendingRestore = null;
    $("restorePanel").hidden = true;
  } catch (error) {
    notice(error.message, true);
  }
});

const fragment = new URLSearchParams(location.hash.slice(1));
if (fragment.has("token")) {
  token = fragment.get("token");
  history.replaceState(null, "", `${location.pathname}#overview`);
}
if (token)
  connect().catch((error) => {
    text("accessError", error.message);
    $("app").hidden = true;
    $("access").hidden = false;
  });
else route();
