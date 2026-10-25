const api = globalThis.browser ?? globalThis.chrome;
const DISCOVERY_URL = "http://127.0.0.1:17653/v1/discover";
const bypassUrls = new Set();

async function settings() {
  return await api.storage.local.get({ enabled: true, ignoreSmallBytes: 0, ignoredDomains: "" });
}
function extensionOrigin() { return api.runtime.getURL("").replace(/\/$/, ""); }
function hostIgnored(url, list) {
  try {
    const host = new URL(url).hostname.toLowerCase();
    return String(list || "").split(/[\n,;]/).map(x => x.trim().toLowerCase().replace(/^\*\./, ""))
      .filter(Boolean).some(domain => host === domain || host.endsWith(`.${domain}`));
  } catch { return true; }
}
async function discover() {
  let response;
  try { response = await fetch(DISCOVERY_URL, { cache: "no-store" }); }
  catch { throw new Error("Pencil is not running. Open the app and try again."); }
  if (!response.ok) throw new Error("Pencil is not running. Open the app and try again.");
  const data = await response.json();
  if (!Number.isInteger(data.port) || data.port < 1 || data.port > 65535) throw new Error("Pencil returned an invalid local endpoint");
  return data.port;
}
async function request(port, token, path, payload) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: path === "/v1/ping" ? "GET" : "POST",
    headers: { "Authorization": `Bearer ${token}`, "Content-Type": "application/json" },
    body: path === "/v1/ping" ? undefined : JSON.stringify(payload),
    cache: "no-store"
  });
  let result = {};
  try { result = await response.json(); } catch { /* non-JSON error body */ }
  return { response, result };
}
// Talks to the app. The app can restart on a different port, so a network failure triggers one
// rediscovery before giving up. A 401 means the pairing was reset in the app, so it is forgotten.
async function callApp(path, payload) {
  const saved = await api.storage.local.get(["token", "port", "origin"]);
  if (!saved.token || !saved.port || !saved.origin) throw new Error("Pair the extension with Pencil first");
  let port = saved.port;
  let attempt;
  try { attempt = await request(port, saved.token, path, payload); }
  catch {
    port = await discover();
    if (port !== saved.port) await api.storage.local.set({ port });
    attempt = await request(port, saved.token, path, payload);
  }
  const { response, result } = attempt;
  if (response.status === 401 || response.status === 403) {
    await api.storage.local.remove(["token", "port", "origin"]);
    throw new Error("Pencil no longer recognises this extension. Pair it again.");
  }
  if (!response.ok || result.ok === false) throw new Error(result.message || "Pencil rejected the request");
  return result;
}
async function cookiesFor(url) {
  try {
    const cookies = await api.cookies.getAll({ url });
    return cookies.map(c => `${c.name}=${c.value}`).join("; ").slice(0, 16384);
  } catch { return ""; }
}
function safeLeaf(value) {
  try {
    const name = decodeURIComponent(String(value || "").replace(/\\/g, "/").split("/").pop() || "");
    return name && name !== "." && name !== ".." && !/[\\/\x00-\x1f]/.test(name) ? name.slice(0, 255) : undefined;
  } catch { return undefined; }
}
function safeName(url) {
  try { return safeLeaf(new URL(url).pathname.split("/").pop()); }
  catch { return undefined; }
}
async function sendDownload(url, referer, fileName) {
  if (!/^https?:\/\//i.test(url)) throw new Error("Only HTTP and HTTPS downloads are supported");
  const item = { url, referer: /^https?:\/\//i.test(referer || "") ? referer : undefined,
    cookies: await cookiesFor(url), user_agent: navigator.userAgent, file_name: fileName || safeName(url) };
  return await callApp("/v1/download", item);
}
async function fallbackDownload(url, filename) {
  bypassUrls.add(url);
  try { return await api.downloads.download({ url, ...(filename ? { filename } : {}) }); }
  finally { setTimeout(() => bypassUrls.delete(url), 10000); }
}

function createContextMenus() {
  for (const [id, title, contexts] of [
    ["pdm-link", "Download link with Pencil Download Manager", ["link"]],
    ["pdm-image", "Download image with Pencil Download Manager", ["image"]],
    ["pdm-media", "Download media with Pencil Download Manager", ["video", "audio"]]
  ]) api.contextMenus.create({ id, title, contexts });
}
api.runtime.onInstalled.addListener(() => {
  if (globalThis.browser) api.contextMenus.removeAll().then(createContextMenus);
  else api.contextMenus.removeAll(createContextMenus);
});

api.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = info.linkUrl || info.srcUrl;
  if (!url || !/^https?:\/\//i.test(url)) return;
  const referer = info.pageUrl || tab?.url || "";
  try { await sendDownload(url, referer, safeName(url)); }
  catch { try { await fallbackDownload(url); } catch { api.notifications.create({ type: "basic", iconUrl: "icon.png", title: "Pencil Download Manager", message: "Could not start this download in Pencil or the browser." }); } }
});

api.downloads.onCreated.addListener(async item => {
  if (!item.url || bypassUrls.has(item.url) || !/^https?:\/\//i.test(item.url)) return;
  const prefs = await settings();
  if (!prefs.enabled || (Number(item.totalBytes) >= 0 && Number(item.totalBytes) < Number(prefs.ignoreSmallBytes || 0)) || hostIgnored(item.url, prefs.ignoredDomains)) return;
  try {
    await sendDownload(item.url, item.referrer || "", safeLeaf(item.filename) || safeName(item.url));
    await api.downloads.cancel(item.id);
  } catch {
    // Deliberately leave the original browser download running if Pencil is unavailable.
  }
});

async function pair(code) {
  const port = await discover();
  const origin = extensionOrigin();
  const response = await fetch(`http://127.0.0.1:${port}/v1/pair`, {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code, origin }), cache: "no-store"
  });
  let result = {};
  try { result = await response.json(); } catch { /* non-JSON error body */ }
  if (!response.ok || !result.token) throw new Error(result.message || "Pairing failed");
  await api.storage.local.set({ token: result.token, port: result.port, origin });
  return { ok: true };
}
async function handleMessage(message) {
  if (message?.type === "pdm-pair") return await pair(String(message.code || "").trim());
  if (message?.type === "pdm-status") {
    try { await callApp("/v1/ping"); return { ok: true, connected: true }; }
    catch (error) { return { ok: true, connected: false, message: String(error?.message || error) }; }
  }
  return { ok: false, message: "Unknown request" };
}
// Chrome ignores a Promise returned from an async listener and only honours sendResponse with
// `return true`, which is why status and pairing always looked broken there. This form works in
// Chrome, Edge and Firefox.
api.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  handleMessage(message).then(sendResponse, error => sendResponse({ ok: false, message: String(error?.message || error) }));
  return true;
});
