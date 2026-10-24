// Placeholder UI: plain HTML/CSS/JS so the project builds without a frontend
// toolchain. Swap for Svelte/React/Solid later; the Rust commands stay the same.
// File names come from remote servers, so everything is inserted with
// textContent (never innerHTML).
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const badge = $("engine-badge");
const message = $("message");
const rows = $("rows");
let ready = false;

function say(text, isError = false) {
  message.textContent = text;
  message.className = isError ? "error" : "";
}

function setBadge(state, detail, command = "") {
  badge.className = "badge " + (state === "missing" ? "error" : state);
  badge.textContent =
    state === "ready" ? "Ready" : state === "error" ? "Engine problem" :
    state === "missing" ? "Setup needed" : "Starting…";
  const needed = $("engine-needed");
  needed.hidden = state !== "missing";
  if (state === "missing") {
    $("engine-needed-detail").textContent = detail || "Install the download engine, then check again.";
    $("engine-command").textContent = command;
    say("");
  } else if (state === "error") {
    say(detail || "The download engine could not start.", true);
  }
  ready = state === "ready";
  $("add-btn").disabled = !ready;
}

function formatBytes(n) {
  if (!n) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return n.toFixed(i === 0 ? 0 : 1) + " " + units[i];
}

function cell(tr, text, cls) {
  const td = document.createElement("td");
  if (cls) td.className = cls;
  td.textContent = text;
  tr.appendChild(td);
  return td;
}

function actionButton(label, fn) {
  const b = document.createElement("button");
  b.textContent = label;
  b.addEventListener("click", async () => {
    try { await fn(); await refresh(); } catch (e) { say(String(e), true); }
  });
  return b;
}

function render(downloads) {
  rows.replaceChildren();
  $("empty").style.display = downloads.length ? "none" : "block";
  for (const d of downloads) {
    const tr = document.createElement("tr");
    if (d.status === "complete") tr.className = "complete";
    if (d.status === "error") tr.className = "error";

    const name = cell(tr, d.name, "name");
    name.title = d.name;
    cell(tr, formatBytes(d.total), "num");

    const pct = d.total ? Math.min(100, (d.completed / d.total) * 100) : 0;
    const prog = document.createElement("td");
    const bar = document.createElement("div");
    bar.className = "bar";
    const fill = document.createElement("span");
    fill.style.width = pct.toFixed(1) + "%";
    bar.appendChild(fill);
    prog.appendChild(bar);
    tr.appendChild(prog);

    cell(tr, d.status === "active" ? formatBytes(d.speed) + "/s" : "—", "num");
    cell(tr, d.error ? d.status + ": " + d.error : d.status);

    const actions = document.createElement("td");
    actions.className = "actions";
    if (d.status === "active" || d.status === "waiting")
      actions.appendChild(actionButton("Pause", () => invoke("pause_download", { gid: d.gid })));
    if (d.status === "paused")
      actions.appendChild(actionButton("Resume", () => invoke("resume_download", { gid: d.gid })));
    actions.appendChild(actionButton("Remove", () => invoke("remove_download", { gid: d.gid })));
    tr.appendChild(actions);

    rows.appendChild(tr);
  }
}

async function refresh() {
  if (!ready) return;
  try { render(await invoke("list_downloads")); } catch (e) { say(String(e), true); }
}

$("add-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const url = $("url").value.trim();
  if (!url) return;
  try {
    await invoke("add_download", { url });
    $("url").value = "";
    say("");
    await refresh();
  } catch (e) {
    say(String(e), true);
  }
});

listen("engine-status", (ev) => {
  setBadge(ev.payload.state, ev.payload.detail, ev.payload.command || "");
  refresh();
});

$("copy-engine-command").addEventListener("click", async () => {
  const command = $("engine-command").textContent;
  if (!command) return;
  try {
    await navigator.clipboard.writeText(command);
    say("Command copied.");
  } catch {
    say("Copy failed. Select the command and copy it manually.", true);
  }
});

$("save-engine-path").addEventListener("click", async () => {
  const path = $("engine-path").value.trim();
  if (!path) {
    say("Enter an absolute executable path.", true);
    return;
  }
  try {
    await invoke("set_engine_path", { path });
    say("Custom path saved. Checking the download engine…");
    await invoke("recheck_engine");
  } catch (e) {
    say(String(e), true);
  }
});

$("recheck-engine").addEventListener("click", async () => {
  $("recheck-engine").disabled = true;
  setBadge("starting");
  try {
    await invoke("recheck_engine");
  } catch (e) {
    say(String(e), true);
  } finally {
    $("recheck-engine").disabled = false;
  }
});
listen("engine-progress", (ev) => {
  const { done, total } = ev.payload;
  say(total ? `Preparing… ${Math.round((done / total) * 100)}%` : "Preparing…");
});

setBadge("starting");
setInterval(refresh, 1000);
