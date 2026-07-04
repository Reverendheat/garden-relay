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

    .split {
      display: grid;
      grid-template-columns: minmax(0, 1fr) minmax(320px, 0.7fr);
      gap: 18px;
      align-items: start;
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
          <button class="action primary" id="save-policy">Save Policy</button>
        </div>
        <div class="split">
          <div id="policies-table"></div>
          <textarea id="policy-editor" spellcheck="false"></textarea>
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
    };

    const policyTemplate = {
      name: "require_tenant",
      phase: "before_model",
      if: { missing_header: "x-garden-tenant" },
      then: { effect: "deny", reason: "Tenant header is required." }
    };

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
      $("policies-table").innerHTML = table(["Name", "Phase", "Effect"], state.policies.map((policy) => [
        `<button class="action" data-policy="${escapeHtml(policy.name)}">${escapeHtml(policy.name)}</button>`,
        escapeHtml(policy.phase),
        escapeHtml(policy.then.effect || "multiple"),
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

    refresh().catch((error) => {
      $("request-detail").textContent = error.message;
    });
  </script>
</body>
</html>"#;
