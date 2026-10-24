<script lang="ts">
  import { onMount } from "svelte";
  import { formatBytes, formatEta, formatPercent, formatSpeed, statusLabel } from "../lib/format";
  import type { Download } from "../lib/types";
  import { tauriApi } from "../lib/api";
  let { download, onclose }: { download: Download; onclose: () => void } = $props();
  let checksum = $state(""); let checksumResult = $state<"pass" | "fail" | "pending" | "">("");
  let action = $state("none"); let downloadLimitKib = $state(0); let programPath = $state(""); let verifying = $state(false); let lastVerified = $state("");
  onMount(() => {
    void (async () => {
    try {
      const settings = Object.fromEntries(await tauriApi.getSettings());
      checksum = settings[`checksum:${download.gid}`] || "";
      action = settings[`post_action:${download.gid}`] || "none";
      programPath = settings[`post_program:${download.gid}`] || "";
    } catch { /* Settings may not be ready yet. */ }
    })();
  });
  async function saveChecksum() {
    try { await tauriApi.setDownloadChecksum(download.gid, checksum); checksumResult = ""; lastVerified = ""; }
    catch (e) { checksumResult = "fail"; }
  }
  async function verify() {
    if (!download.path || !checksum.trim()) return;
    verifying = true;
    try { checksumResult = await tauriApi.verifyChecksum(download.path, checksum) ? "pass" : "fail"; lastVerified = `${download.path}:${checksum}`; }
    catch { checksumResult = "fail"; }
    finally { verifying = false; }
  }
  $effect(() => { if (download.status === "complete" && checksum.trim() && lastVerified !== `${download.path}:${checksum}` && !verifying) void verify(); });
  async function saveAction() {
    try {
      if (action === "run-program") {
        if (!window.confirm("Allow this exact program to run automatically after this download completes? It will be launched directly without a shell.")) return;
        await tauriApi.setPostDownloadProgram(download.gid, programPath, true);
      } else await tauriApi.setPostDownloadAction(download.gid, action);
    } catch (e) { checksumResult = "fail"; }
  }
</script>
<aside class="details" aria-label="Download details">
  <header><h2>{download.name}</h2><button type="button" onclick={onclose} aria-label="Close details">✕</button></header>
  <dl>
    <dt>Status</dt><dd>{statusLabel(download.status)}</dd>
    <dt>Progress</dt><dd>{#if download.percent !== null}<progress max="100" value={download.percent} aria-label="Download progress"></progress>{/if}{formatPercent(download.percent)}</dd>
    <dt>Speed</dt><dd>{formatSpeed(download.smoothSpeed)}</dd>
    <dt>ETA</dt><dd>{formatEta(download.eta)}</dd>
    <dt>Size</dt><dd>{formatBytes(download.completed)} of {formatBytes(download.total)}</dd>
    <dt>Path</dt><dd>{download.path || "—"}</dd>
    {#if download.error}<dt>Error</dt><dd class="err">{download.error}</dd>{/if}
  </dl>
  <section class="download-options">
    <h3>Verify checksum</h3>
    <label>SHA-256, SHA-1, or MD5 <input value={checksum} oninput={() => { checksumResult = ""; }} onchange={saveChecksum} placeholder="Paste checksum" /></label>
    {#if download.status === "complete" && checksum}<p class="checksum-badge" class:pass={checksumResult === "pass"} class:fail={checksumResult === "fail"}>{checksumResult === "pass" ? "Checksum passed" : checksumResult === "fail" ? "Checksum failed" : verifying ? "Checking…" : "Not checked"}</p>{/if}
    <h3>Per-download speed limit</h3>
    <label>KiB/s (0 = no limit) <input type="number" min="0" max="1048576" bind:value={downloadLimitKib} /></label>
    <button type="button" onclick={async () => { try { await tauriApi.setDownloadSpeedLimit(download.gid, downloadLimitKib > 0 ? Math.round(downloadLimitKib * 1024) : null); } catch (e) { checksumResult = "fail"; } }}>Apply limit</button>
    <h3>After download</h3>
    <label>Action <select bind:value={action} onchange={() => { if (action !== "run-program") void saveAction(); }}><option value="none">None</option><option value="open-folder">Open folder</option><option value="sound">Play a sound</option><option value="notification">Show a notification</option><option value="run-program">Run an approved program</option><option value="sleep">Sleep (60-second countdown)</option><option value="hibernate">Hibernate (60-second countdown)</option><option value="shutdown">Shut down (60-second countdown)</option></select></label>
    {#if action === "run-program"}<label>Program path <input bind:value={programPath} placeholder="Absolute path to program" /></label><button type="button" onclick={saveAction}>Confirm and allow program</button>{/if}
  </section>
</aside>
