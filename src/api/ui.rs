use axum::response::Html;

pub async fn admin_ui() -> Html<&'static str> {
    Html(ADMIN_UI)
}

const ADMIN_UI: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Garden Relay</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f7f7f4;
      --ink: #202421;
      --muted: #626a65;
      --line: #d8ddd7;
      --panel: #ffffff;
      --accent: #1c7c74;
      --accent-strong: #105d57;
      --warn: #a86505;
      --danger: #a33b31;
      --ok: #287143;
    }

    * { box-sizing: border-box; }

    body {
      margin: 0;
      background: var(--bg);
      color: var(--ink);
      font: 14px/1.4 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }

    header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding: 18px 24px;
      border-bottom: 1px solid var(--line);
      background: var(--panel);
      position: sticky;
      top: 0;
      z-index: 2;
    }

    h1 {
      margin: 0;
      font-size: 18px;
      font-weight: 700;
    }

    main {
      display: grid;
      grid-template-columns: 280px minmax(0, 1fr);
      min-height: calc(100vh - 62px);
    }

    nav {
      border-right: 1px solid var(--line);
      padding: 18px;
      background: #fbfbf8;
    }

    nav button, .action {
      border: 1px solid var(--line);
      background: var(--panel);
      color: var(--ink);
      border-radius: 6px;
      min-height: 34px;
      padding: 7px 10px;
      font: inherit;
      cursor: pointer;
    }

    nav button {
      display: flex;
      width: 100%;
      align-items: center;
      justify-content: space-between;
      margin-bottom: 8px;
      text-align: left;
    }

    nav button.active {
      border-color: var(--accent);
      color: var(--accent-strong);
      font-weight: 650;
    }

    .content {
      padding: 22px 24px 42px;
      min-width: 0;
    }

    .toolbar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      margin-bottom: 14px;
    }

    h2 {
      margin: 0;
      font-size: 16px;
    }

    .grid {
      display: grid;
      grid-template-columns: minmax(0, 1fr);
      gap: 18px;
    }

    table {
      width: 100%;
      border-collapse: collapse;
      background: var(--panel);
      border: 1px solid var(--line);
    }

    th, td {
      border-bottom: 1px solid var(--line);
      padding: 10px 12px;
      text-align: left;
      vertical-align: top;
    }

    th {
      color: var(--muted);
      font-size: 12px;
      font-weight: 650;
      text-transform: uppercase;
    }

    tr:last-child td { border-bottom: 0; }

    tr.selected-row td {
      background: var(--ink);
      color: #fff;
      border-bottom-color: var(--ink);
    }

    tr.selected-row .action {
      background: #fff;
      border-color: #fff;
      color: var(--ink);
      font-weight: 700;
    }

    tr.selected-row .status {
      background: transparent;
      border-color: rgba(255, 255, 255, 0.55);
      color: #fff;
    }

    code, pre, textarea {
      font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    }

    pre {
      overflow: auto;
      max-height: 340px;
      margin: 0;
      padding: 12px;
      border: 1px solid var(--line);
      background: #111513;
      color: #ecf1ed;
      border-radius: 6px;
    }

    textarea {
      width: 100%;
      min-height: 220px;
      resize: vertical;
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 12px;
      background: var(--panel);
      color: var(--ink);
    }

    input, select {
      width: 100%;
      min-height: 34px;
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 6px 8px;
      background: var(--panel);
      color: var(--ink);
      font: inherit;
    }

    label {
      display: grid;
      gap: 5px;
      color: var(--muted);
      font-size: 12px;
      font-weight: 650;
    }

    label span {
      color: var(--muted);
    }

    .split {
      display: grid;
      grid-template-columns: minmax(0, 1fr) minmax(320px, 0.7fr);
      gap: 18px;
      align-items: start;
    }

    .policy-layout {
      display: grid;
      grid-template-columns: minmax(0, 1fr);
      gap: 18px;
      align-items: start;
    }

    .policy-editor-layout {
      display: grid;
      grid-template-columns: minmax(320px, 1fr) minmax(320px, 1fr);
      gap: 18px;
      align-items: start;
    }

    .builder {
      display: grid;
      gap: 14px;
      padding: 14px;
      border: 1px solid var(--line);
      background: var(--panel);
    }

    .builder-section {
      display: grid;
      gap: 10px;
    }

    .builder-section-title {
      color: var(--ink);
      font-size: 12px;
      font-weight: 750;
      text-transform: uppercase;
    }

    .field-grid {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 10px;
    }

    .field-grid .wide {
      grid-column: 1 / -1;
    }

    .editor-stack {
      display: grid;
      gap: 10px;
    }

    .status {
      display: inline-flex;
      align-items: center;
      min-height: 24px;
      padding: 2px 8px;
      border-radius: 999px;
      font-size: 12px;
      border: 1px solid var(--line);
      background: #f4f6f3;
    }

    .status.pending { color: var(--warn); }
    .status.approved { color: var(--ok); }
    .status.denied, .status.failed { color: var(--danger); }
    .status.completed { color: var(--ok); }
    .status.consumed { color: var(--muted); }

    .row-actions {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }

    .primary {
      background: var(--accent);
      border-color: var(--accent);
      color: #fff;
    }

    .danger {
      border-color: #e0b7b2;
      color: var(--danger);
    }

    .muted { color: var(--muted); }
    .hidden { display: none; }

    @media (max-width: 860px) {
      main { grid-template-columns: 1fr; }
      nav {
        display: flex;
        gap: 8px;
        overflow-x: auto;
        border-right: 0;
        border-bottom: 1px solid var(--line);
      }
      nav button { min-width: 160px; margin-bottom: 0; }
      .split { grid-template-columns: 1fr; }
      .policy-layout { grid-template-columns: 1fr; }
      .policy-editor-layout { grid-template-columns: 1fr; }
      header { align-items: flex-start; flex-direction: column; }
    }
  </style>
</head>
<body>
  <header>
    <h1>Garden Relay</h1>
    <button class="action" id="refresh">Refresh</button>
  </header>
  <main>
    <nav>
      <button data-view="requests" class="active">Requests <span id="request-count">0</span></button>
      <button data-view="policies">Policies <span id="policy-count">0</span></button>
      <button data-view="approvals">Approvals <span id="approval-count">0</span></button>
    </nav>
    <section class="content">
      <section id="requests-view">
        <div class="toolbar">
          <h2>Recent Requests</h2>
          <span class="muted" id="requests-updated"></span>
        </div>
        <div class="split">
          <div id="requests-table"></div>
          <pre id="request-detail">{}</pre>
        </div>
      </section>

      <section id="policies-view" class="hidden">
        <div class="toolbar">
          <h2>Policies</h2>
          <div class="row-actions">
            <button class="action" id="builder-from-json">Load JSON</button>
            <button class="action" id="builder-to-json">Build JSON</button>
            <button class="action primary" id="save-policy">Save Policy</button>
          </div>
        </div>
        <div class="policy-layout">
          <div id="policies-table"></div>
          <div class="policy-editor-layout">
            <div class="builder" id="policy-builder">
              <div class="builder-section">
                <div class="builder-section-title">Policy</div>
                <div class="field-grid">
                  <label class="wide"><span>Name</span><input id="builder-name" type="text"></label>
                  <label><span>Phase</span><select id="builder-phase"></select></label>
                </div>
              </div>
              <div class="builder-section">
                <div class="builder-section-title">Condition</div>
                <div class="field-grid">
                  <label class="wide"><span>Field</span><select id="builder-condition"></select></label>
                  <div class="wide field-grid" id="builder-condition-fields"></div>
                </div>
              </div>
              <div class="builder-section">
                <div class="builder-section-title">Effects</div>
                <div id="builder-effects-table"></div>
                <div class="row-actions">
                  <button class="action" id="builder-add-effect">Add Effect</button>
                  <button class="action danger" id="builder-remove-effect">Remove Effect</button>
                </div>
                <div class="field-grid">
                  <label class="wide"><span>Selected effect</span><select id="builder-effect"></select></label>
                </div>
                <div class="field-grid" id="builder-effect-fields"></div>
              </div>
            </div>
            <div class="editor-stack">
              <textarea id="policy-editor" spellcheck="false"></textarea>
            </div>
          </div>
        </div>
      </section>

      <section id="approvals-view" class="hidden">
        <div class="toolbar">
          <h2>Approvals</h2>
          <span class="muted" id="approvals-updated"></span>
        </div>
        <div class="split">
          <div id="approvals-table"></div>
          <pre id="approval-detail">{}</pre>
        </div>
      </section>
    </section>
  </main>

  <script>
    const state = {
      requests: [],
      policies: [],
      approvals: [],
      activeView: "requests",
      selectedRequestId: null,
      builderEffects: [],
      selectedEffectIndex: 0,
    };

    const policyTemplate = {
      name: "require_tenant",
      phase: "before_model",
      if: { missing_header: "x-garden-tenant" },
      then: { effect: "deny", reason: "Tenant header is required." }
    };

    const phaseOptions = [
      ["before_input", "before_input"],
      ["before_model", "before_model"],
      ["after_model", "after_model"],
      ["before_response", "before_response"],
    ];

    const conditionOptions = [
      ["always", "always"],
      ["missing_header", "missing_header"],
      ["header_equals", "header_equals"],
      ["model", "model"],
      ["tenant_id", "tenant_id"],
      ["app_id", "app_id"],
      ["user_id", "user_id"],
      ["input_contains", "input_contains"],
      ["request_json_equals", "request_json_equals"],
      ["response_contains", "response_contains"],
      ["response_json_equals", "response_json_equals"],
      ["estimated_input_tokens_greater_than", "estimated_input_tokens_greater_than"],
      ["tool_name", "tool_name"],
    ];

    const effectOptions = [
      ["deny", "deny"],
      ["log", "log"],
      ["disable_tools", "disable_tools"],
      ["augment", "augment"],
      ["require_approval", "require_approval"],
    ];

    const $ = (id) => document.getElementById(id);
    const pretty = (value) => JSON.stringify(value, null, 2);

    async function fetchJson(path, options) {
      const response = await fetch(path, options);
      if (!response.ok) {
        const text = await response.text();
        throw new Error(text || response.statusText);
      }
      return response.json();
    }

    async function refresh() {
      const [requests, policies, approvals] = await Promise.all([
        fetchJson("/v1/requests"),
        fetchJson("/v1/policies"),
        fetchJson("/v1/approvals"),
      ]);
      state.requests = requests;
      state.policies = policies;
      state.approvals = approvals;
      if (!state.requests.some((request) => request.request_id === state.selectedRequestId)) {
        state.selectedRequestId = state.requests[0]?.request_id || null;
      }
      render();
    }

    function render() {
      $("request-count").textContent = state.requests.length;
      $("policy-count").textContent = state.policies.length;
      $("approval-count").textContent = state.approvals.filter((approval) => approval.status === "pending").length;
      renderRequests();
      renderPolicies();
      renderApprovals();
    }

    function renderRequests() {
      $("requests-updated").textContent = new Date().toLocaleTimeString();
      if (!state.requests.length) {
        $("requests-table").innerHTML = table(["Request", "Model", "Tenant", "Status"], []);
        $("request-detail").textContent = "{}";
        return;
      }

      $("requests-table").innerHTML = table(["Request", "Model", "Tenant", "Status"], state.requests.map((request) => {
        const relay = request.relay_request || {};
        const meta = relay.metadata || {};
        const status = request.outcome ? request.outcome.status : "running";
        return {
          rowClass: request.request_id === state.selectedRequestId ? "selected-row" : "",
          cells: [
            `<button class="action" data-request="${request.request_id}">${shortId(request.request_id)}</button>`,
            escapeHtml(relay.model || ""),
            escapeHtml(meta.tenant_id || ""),
            `<span class="status ${escapeHtml(status)}">${escapeHtml(status)}</span>`
          ],
        };
      }));
      const selectedRequest = state.requests.find((request) => request.request_id === state.selectedRequestId) || state.requests[0];
      $("request-detail").textContent = pretty(selectedRequest);
    }

    function renderPolicies() {
      $("policy-editor").value ||= pretty(state.policies[0] || policyTemplate);
      fillBuilderFromPolicy(parsePolicyEditor() || state.policies[0] || policyTemplate);
      $("policies-table").innerHTML = table(["Name", "Phase", "Effect"], state.policies.map((policy) => [
        `<button class="action" data-policy="${escapeHtml(policy.name)}">${escapeHtml(policy.name)}</button>`,
        escapeHtml(policy.phase),
        escapeHtml(effectSummary(policy.then)),
      ]));
    }

    function renderApprovals() {
      $("approvals-updated").textContent = new Date().toLocaleTimeString();
      if (!state.approvals.length) {
        $("approvals-table").innerHTML = table(["Approval", "Policy", "Status", "Actions"], []);
        $("approval-detail").textContent = "{}";
        return;
      }

      $("approvals-table").innerHTML = table(["Approval", "Policy", "Status", "Actions"], state.approvals.map((approval) => [
        `<button class="action" data-approval="${approval.approval_id}">${shortId(approval.approval_id)}</button>`,
        escapeHtml(approval.policy_name),
        `<span class="status ${escapeHtml(approval.status)}">${escapeHtml(approval.status)}</span>`,
        approval.status === "pending"
          ? `<div class="row-actions"><button class="action primary" data-approve="${approval.approval_id}">Approve</button><button class="action danger" data-deny="${approval.approval_id}">Deny</button></div>`
          : "",
      ]));
      $("approval-detail").textContent = pretty(state.approvals[0]);
    }

    function renderBuilderOptions() {
      $("builder-phase").innerHTML = optionsHtml(phaseOptions);
      $("builder-condition").innerHTML = optionsHtml(conditionOptions);
      $("builder-effect").innerHTML = optionsHtml(effectOptions);
      renderConditionFields();
      state.builderEffects = [clone(policyTemplate.then)];
      state.selectedEffectIndex = 0;
      fillSelectedEffectFields();
    }

    function renderConditionFields() {
      const type = $("builder-condition").value;
      const fields = $("builder-condition-fields");
      if (type === "always") {
        fields.innerHTML = `<label><span>Value</span><select id="builder-condition-value"><option value="true">true</option><option value="false">false</option></select></label>`;
      } else if (type === "header_equals") {
        fields.innerHTML = `${inputHtml("builder-condition-name", "Header")} ${inputHtml("builder-condition-value", "Value")}`;
      } else if (type === "request_json_equals" || type === "response_json_equals") {
        fields.innerHTML = `${inputHtml("builder-condition-pointer", "Pointer")} ${inputHtml("builder-condition-json", "Value", "wide")}`;
      } else {
        fields.innerHTML = inputHtml("builder-condition-value", "Value", "wide", type === "estimated_input_tokens_greater_than" ? "number" : "text");
      }
    }

    function renderEffectFields() {
      const effect = $("builder-effect").value;
      const fields = $("builder-effect-fields");
      if (effect === "log") {
        fields.innerHTML = `
          <label><span>Level</span><select id="builder-effect-level"><option value="trace">trace</option><option value="debug">debug</option><option value="info" selected>info</option><option value="warn">warn</option><option value="error">error</option></select></label>
          ${inputHtml("builder-effect-message", "Message")}
        `;
      } else if (effect === "disable_tools") {
        fields.innerHTML = inputHtml("builder-effect-tools", "Tools", "wide");
      } else if (effect === "augment") {
        fields.innerHTML = `
          <label class="wide"><span>Augment mode</span><select id="builder-effect-mode"><option value="append">append</option><option value="prepend">prepend</option><option value="replace">replace</option></select></label>
          <label><span>Role</span><select id="builder-effect-role"><option value="system">system</option><option value="developer">developer</option><option value="user">user</option><option value="assistant">assistant</option><option value="tool">tool</option></select></label>
          ${inputHtml("builder-effect-content", "Content", "wide")}
        `;
      } else {
        fields.innerHTML = inputHtml("builder-effect-reason", "Reason", "wide");
      }
    }

    function renderEffectsList() {
      $("builder-effects-table").innerHTML = table(["Effect", "Details"], state.builderEffects.map((effect, index) => ({
        rowClass: index === state.selectedEffectIndex ? "selected-row" : "",
        cells: [
          `<button class="action" data-builder-effect-index="${index}">${escapeHtml(effect.effect || "effect")}</button>`,
          escapeHtml(effectDetails(effect)),
        ],
      })));
    }

    function inputHtml(id, label, className = "", type = "text") {
      return `<label class="${className}"><span>${label}</span><input id="${id}" type="${type}"></label>`;
    }

    function optionsHtml(options) {
      return options.map(([value, label]) => `<option value="${value}">${label}</option>`).join("");
    }

    function fillBuilderFromPolicy(policy) {
      if (!$("builder-name")) return;

      $("builder-name").value = policy?.name || policyTemplate.name;
      $("builder-phase").value = policy?.phase || policyTemplate.phase;

      const condition = policy?.if || policyTemplate.if;
      const conditionType = firstKnownKey(condition, conditionOptions.map(([value]) => value)) || "missing_header";
      $("builder-condition").value = conditionType;
      renderConditionFields();
      fillConditionFields(conditionType, condition?.[conditionType]);

      state.builderEffects = effectsFromPolicy(policy);
      state.selectedEffectIndex = 0;
      fillSelectedEffectFields();
    }

    function fillConditionFields(type, value) {
      if (type === "always") {
        $("builder-condition-value").value = String(value ?? true);
      } else if (type === "header_equals") {
        $("builder-condition-name").value = value?.name || "";
        $("builder-condition-value").value = value?.value || "";
      } else if (type === "request_json_equals" || type === "response_json_equals") {
        $("builder-condition-pointer").value = value?.pointer || "";
        $("builder-condition-json").value = value?.value === undefined ? "" : JSON.stringify(value.value);
      } else {
        $("builder-condition-value").value = value ?? "";
      }
    }

    function fillEffectFields(effect) {
      if (effect.effect === "log") {
        $("builder-effect-level").value = effect.level || "info";
        $("builder-effect-message").value = effect.message || effect.reason || "";
      } else if (effect.effect === "disable_tools") {
        $("builder-effect-tools").value = (effect.tools || []).join(", ");
      } else if (effect.effect === "augment") {
        const message = effect.messages?.[0] || {};
        $("builder-effect-mode").value = effect.mode || "append";
        $("builder-effect-role").value = message.role || "system";
        $("builder-effect-content").value = message.content || "";
      } else {
        $("builder-effect-reason").value = effect.reason || "";
      }
    }

    function fillSelectedEffectFields() {
      const effect = selectedBuilderEffect();
      $("builder-effect").value = effect.effect || policyTemplate.then.effect;
      renderEffectFields();
      fillEffectFields(effect);
      renderEffectsList();
    }

    function buildPolicyFromBuilder() {
      updateSelectedEffectFromFields();
      const conditionType = $("builder-condition").value;
      return {
        name: $("builder-name").value.trim() || policyTemplate.name,
        phase: $("builder-phase").value,
        if: buildCondition(conditionType),
        then: buildAction(),
      };
    }

    function syncPolicyEditorFromBuilder() {
      $("policy-editor").value = pretty(buildPolicyFromBuilder());
    }

    function buildCondition(type) {
      if (type === "always") {
        return { always: $("builder-condition-value").value === "true" };
      }
      if (type === "header_equals") {
        return { header_equals: { name: $("builder-condition-name").value.trim(), value: $("builder-condition-value").value } };
      }
      if (type === "request_json_equals" || type === "response_json_equals") {
        return {
          [type]: {
            pointer: $("builder-condition-pointer").value.trim(),
            value: parseJsonValue($("builder-condition-json").value),
          },
        };
      }
      if (type === "estimated_input_tokens_greater_than") {
        return { [type]: Number($("builder-condition-value").value || 0) };
      }
      return { [type]: $("builder-condition-value").value };
    }

    function buildEffect(effect) {
      if (effect === "log") {
        return {
          effect,
          level: $("builder-effect-level").value,
          message: $("builder-effect-message").value,
        };
      }
      if (effect === "disable_tools") {
        return {
          effect,
          tools: splitList($("builder-effect-tools").value),
        };
      }
      if (effect === "augment") {
        return {
          effect,
          mode: $("builder-effect-mode").value,
          messages: [{
            role: $("builder-effect-role").value,
            content: $("builder-effect-content").value,
          }],
        };
      }
      return {
        effect,
        reason: $("builder-effect-reason").value,
      };
    }

    function buildAction() {
      const effects = state.builderEffects.length ? state.builderEffects.map(clone) : [clone(policyTemplate.then)];
      if (effects.length === 1) {
        return effects[0];
      }
      return { effects };
    }

    function updateSelectedEffectFromFields() {
      if (!$("builder-effect") || !state.builderEffects.length) return;
      state.builderEffects[state.selectedEffectIndex] = buildEffect($("builder-effect").value);
      renderEffectsList();
    }

    function selectedBuilderEffect() {
      if (!state.builderEffects.length) {
        state.builderEffects = [clone(policyTemplate.then)];
      }
      if (state.selectedEffectIndex >= state.builderEffects.length) {
        state.selectedEffectIndex = state.builderEffects.length - 1;
      }
      return state.builderEffects[state.selectedEffectIndex];
    }

    function effectsFromPolicy(policy) {
      if (Array.isArray(policy?.then?.effects) && policy.then.effects.length) {
        return policy.then.effects.map(clone);
      }
      if (policy?.then?.effect) {
        return [clone(policy.then)];
      }
      return [clone(policyTemplate.then)];
    }

    function defaultEffectFor(effect) {
      if (effect === "log") {
        return { effect, level: "info", message: "Policy matched." };
      }
      if (effect === "disable_tools") {
        return { effect, tools: [] };
      }
      if (effect === "augment") {
        return { effect, mode: "append", messages: [{ role: "system", content: "" }] };
      }
      if (effect === "require_approval") {
        return { effect, reason: "Human approval is required." };
      }
      return { effect, reason: "Request denied by policy." };
    }

    function effectSummary(action) {
      if (Array.isArray(action?.effects)) {
        return action.effects.map((effect) => effect.effect).join(", ");
      }
      return action?.effect || "";
    }

    function effectDetails(effect) {
      if (effect.effect === "log") {
        return effect.message || effect.reason || effect.level || "";
      }
      if (effect.effect === "disable_tools") {
        return (effect.tools || []).join(", ");
      }
      if (effect.effect === "augment") {
        const messages = effect.messages?.map((message) => `${message.role}: ${message.content}`).join(" | ") || "";
        return `${effect.mode || "append"} ${messages}`.trim();
      }
      return effect.reason || "";
    }

    function clone(value) {
      return JSON.parse(JSON.stringify(value));
    }

    function parsePolicyEditor() {
      try {
        return JSON.parse($("policy-editor").value);
      } catch {
        return null;
      }
    }

    function parseJsonValue(value) {
      if (!value.trim()) return null;
      try {
        return JSON.parse(value);
      } catch {
        return value;
      }
    }

    function splitList(value) {
      return value.split(",").map((item) => item.trim()).filter(Boolean);
    }

    function firstKnownKey(value, keys) {
      return keys.find((key) => Object.prototype.hasOwnProperty.call(value || {}, key));
    }

    function table(headers, rows) {
      return `<table><thead><tr>${headers.map((header) => `<th>${header}</th>`).join("")}</tr></thead><tbody>${
        rows.length
          ? rows.map((row) => {
              const cells = Array.isArray(row) ? row : row.cells;
              const rowClass = Array.isArray(row) ? "" : row.rowClass;
              return `<tr class="${escapeHtml(rowClass || "")}">${cells.map((cell) => `<td>${cell}</td>`).join("")}</tr>`;
            }).join("")
          : `<tr><td colspan="${headers.length}" class="muted">No records</td></tr>`
      }</tbody></table>`;
    }

    function shortId(id) {
      return id ? id.slice(0, 8) : "";
    }

    function escapeHtml(value) {
      return String(value)
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;")
        .replaceAll('"', "&quot;")
        .replaceAll("'", "&#039;");
    }

    document.addEventListener("click", async (event) => {
      const target = event.target.closest("button");
      if (!target) return;

      if (target.dataset.view) {
        state.activeView = target.dataset.view;
        document.querySelectorAll("nav button").forEach((button) => button.classList.toggle("active", button.dataset.view === state.activeView));
        ["requests", "policies", "approvals"].forEach((view) => $(`${view}-view`).classList.toggle("hidden", view !== state.activeView));
      }

      if (target.id === "refresh") {
        await refresh();
      }

      if (target.dataset.request) {
        const request = state.requests.find((item) => item.request_id === target.dataset.request);
        state.selectedRequestId = request?.request_id || null;
        $("request-detail").textContent = pretty(request || {});
        renderRequests();
      }

      if (target.dataset.policy) {
        const policy = state.policies.find((item) => item.name === target.dataset.policy);
        $("policy-editor").value = pretty(policy || policyTemplate);
        fillBuilderFromPolicy(policy || policyTemplate);
      }

      if (target.id === "builder-to-json") {
        $("policy-editor").value = pretty(buildPolicyFromBuilder());
      }

      if (target.id === "builder-from-json") {
        fillBuilderFromPolicy(parsePolicyEditor() || policyTemplate);
      }

      if (target.dataset.builderEffectIndex) {
        updateSelectedEffectFromFields();
        state.selectedEffectIndex = Number(target.dataset.builderEffectIndex);
        fillSelectedEffectFields();
        syncPolicyEditorFromBuilder();
      }

      if (target.id === "builder-add-effect") {
        updateSelectedEffectFromFields();
        state.builderEffects.push(defaultEffectFor("log"));
        state.selectedEffectIndex = state.builderEffects.length - 1;
        fillSelectedEffectFields();
        syncPolicyEditorFromBuilder();
      }

      if (target.id === "builder-remove-effect") {
        if (state.builderEffects.length > 1) {
          state.builderEffects.splice(state.selectedEffectIndex, 1);
          state.selectedEffectIndex = Math.max(0, state.selectedEffectIndex - 1);
          fillSelectedEffectFields();
          syncPolicyEditorFromBuilder();
        }
      }

      if (target.id === "save-policy") {
        const policy = JSON.parse($("policy-editor").value);
        await fetchJson("/v1/policies", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(policy),
        });
        await refresh();
      }

      if (target.dataset.approval) {
        const approval = state.approvals.find((item) => item.approval_id === target.dataset.approval);
        $("approval-detail").textContent = pretty(approval || {});
      }

      if (target.dataset.approve) {
        await fetchJson(`/v1/approvals/${target.dataset.approve}/approve`, { method: "POST" });
        await refresh();
      }

      if (target.dataset.deny) {
        await fetchJson(`/v1/approvals/${target.dataset.deny}/deny`, { method: "POST" });
        await refresh();
      }
    });

    document.addEventListener("change", (event) => {
      if (event.target.id === "builder-condition") {
        renderConditionFields();
      }
      if (event.target.id === "builder-effect") {
        state.builderEffects[state.selectedEffectIndex] = defaultEffectFor(event.target.value);
        renderEffectFields();
        fillEffectFields(selectedBuilderEffect());
      }
      if (event.target.id.startsWith("builder-") && event.target.id !== "builder-from-json" && event.target.id !== "builder-to-json") {
        syncPolicyEditorFromBuilder();
      }
    });

    document.addEventListener("input", (event) => {
      if (event.target.id.startsWith("builder-")) {
        syncPolicyEditorFromBuilder();
      }
    });

    renderBuilderOptions();
    refresh().catch((error) => {
      $("request-detail").textContent = error.message;
    });
  </script>
</body>
</html>"#;
