const api = globalThis.browser ?? globalThis.chrome;
const $ = id => document.getElementById(id);
async function refresh() {
  const { enabled, ignoreSmallBytes, ignoredDomains } = await api.storage.local.get({ enabled: true, ignoreSmallBytes: 0, ignoredDomains: "" });
  $("enabled").checked = enabled; $("small").value = String(ignoreSmallBytes); $("domains").value = ignoredDomains;
  const status = await api.runtime.sendMessage({ type: "pdm-status" });
  $("status").textContent = status?.connected ? "Connected to Pencil" : "Not connected — open Pencil and pair below.";
  $("status").className = status?.connected ? "connected" : "disconnected";
  $("pairing").hidden = Boolean(status?.connected);
}
$("enabled").addEventListener("change", () => api.storage.local.set({ enabled: $("enabled").checked }));
$("save").addEventListener("click", async () => { await api.storage.local.set({ enabled: $("enabled").checked, ignoreSmallBytes: Number($("small").value), ignoredDomains: $("domains").value }); $("message").textContent = "Settings saved."; });
$("pair").addEventListener("click", async () => { $("message").textContent = "Pairing…"; try { await api.runtime.sendMessage({ type: "pdm-pair", code: $("code").value.trim() }); $("message").textContent = "Paired successfully."; await refresh(); } catch (e) { $("message").textContent = String(e); } });
refresh().catch(e => { $("status").textContent = String(e); });
