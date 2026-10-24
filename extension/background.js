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
  const response = await fetch(DISCOVERY_URL, { cache: "no-store" });
  if (!response.ok) throw new Error("Pencil is not running");
  const data = await response.json();
  if (!Number.isInteger(data.port) || data.port < 1 || data.port > 65535) throw new Error("Invalid local endpoint");
  return data.port;
}
async function callApp(path, payload) {
  const { token, port, origin } = await api.storage.local.get(["token", "port", "origin"]);
  if (!token || !port || !origin) throw new Error("Pair the extension first");
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: path === "/v1/ping" ? "GET" : "POST",
    headers: { "Authorization": `Bearer ${token}`, "Content-Type": "application/json" },
    body: path === "/v1/ping" ? undefined : JSON.stringify(payload),
    cache: "no-store"
  });
  const result = await response.json();
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

api.runtime.onMessage.addListener(async message => {
  if (message?.type === "pdm-pair") {
    const port = await discover();
    const origin = extensionOrigin();
    const response = await fetch(`http://127.0.0.1:${port}/v1/pair`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code: message.code, origin }), cache: "no-store"
    });
    const result = await response.json();
    if (!response.ok || !result.token) throw new Error(result.message || "Pairing failed");
    await api.storage.local.set({ token: result.token, port: result.port, origin });
    return { ok: true };
  }
  if (message?.type === "pdm-status") {
    try { await callApp("/v1/ping"); return { connected: true }; }
    catch (error) { return { connected: false, message: String(error) }; }
  }
});
