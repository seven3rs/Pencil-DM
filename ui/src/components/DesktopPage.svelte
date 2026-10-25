<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { tauriApi, type Aria2About } from "../lib/api";
  import type { DownloadStore } from "../lib/store.svelte";

  let { store, onclose, oncheckupdate }: { store: DownloadStore; onclose: () => void; oncheckupdate: () => void } = $props();
  let minimizeToTray = $state(true);
  let clipboardMonitor = $state(false);
  let startWithOs = $state(false);
  let checkUpdatesOnStartup = $state(true);
  let appVersion = $state("...");
  let loaded = $state(false);
  let busy = $state(false);
  let aria2 = $state<Aria2About | null>(null);
  let confirmDelete = $state(false);

  onMount(() => {
    void tauriApi.aria2About().then((info) => aria2 = info).catch(() => {});
    void getVersion().then((version) => appVersion = version).catch(() => {});
    void (async () => {
      try {
        const settings = Object.fromEntries(await tauriApi.getSettings());
        minimizeToTray = settings.minimize_to_tray !== "false";
        clipboardMonitor = settings.clipboard_monitor_enabled === "true";
        startWithOs = await tauriApi.getAutostartEnabled().catch(() => false);
        checkUpdatesOnStartup = settings.check_updates_on_startup !== "false";
      } catch (error) {
        store.say(String(error), true);
      } finally {
        loaded = true;
      }
    })();
  });

  async function saveBoolean(key: string, value: boolean) {
    try {
      await tauriApi.setSetting(key, value ? "true" : "false");
      window.dispatchEvent(new Event("desktop-settings-changed"));
    } catch (error) {
      store.say(String(error), true);
      if (key === "minimize_to_tray") minimizeToTray = !value;
      if (key === "clipboard_monitor_enabled") clipboardMonitor = !value;
      if (key === "check_updates_on_startup") checkUpdatesOnStartup = !value;
      return;
    }
  }

  async function toggleAutostart(value: boolean) {
    try {
      await tauriApi.setAutostartEnabled(value);
      startWithOs = value;
    } catch (error) {
      store.say(String(error), true);
      startWithOs = !value;
    }
  }

  async function copyDiagnostics() {
    busy = true;
    try {
      const report = await tauriApi.diagnosticsReport();
      await tauriApi.writeClipboard(report);
      store.say("Diagnostics report copied. URLs, paths, and file names are redacted.");
    } catch (error) {
      store.say(String(error), true);
    } finally {
      busy = false;
    }
  }

  async function deleteAllData() {
    busy = true;
    try {
      await tauriApi.deleteAllAppData(true);
    } catch (error) {
      store.say(String(error), true);
      confirmDelete = false;
    } finally {
      busy = false;
    }
  }
</script>

<section class="desktop-page" aria-labelledby="about-title">
  <header class="page-heading">
    <div><h2 id="about-title">About</h2><p>Desktop integration and diagnostics</p></div>
    <button type="button" onclick={onclose}>Back to downloads</button>
  </header>

  <section class="settings-card">
    <h3>Desktop behavior</h3>
    <label><input type="checkbox" bind:checked={minimizeToTray} disabled={!loaded} onchange={(event) => { minimizeToTray = event.currentTarget.checked; void saveBoolean("minimize_to_tray", event.currentTarget.checked); }} /> Minimize to system tray when closing the window (default on)</label>
    <label><input type="checkbox" bind:checked={clipboardMonitor} disabled={!loaded} onchange={(event) => { clipboardMonitor = event.currentTarget.checked; void saveBoolean("clipboard_monitor_enabled", event.currentTarget.checked); }} /> Clipboard link prompt (default off)</label>
    <label><input type="checkbox" bind:checked={checkUpdatesOnStartup} disabled={!loaded} onchange={(event) => { checkUpdatesOnStartup = event.currentTarget.checked; void saveBoolean("check_updates_on_startup", event.currentTarget.checked); }} /> Check for updates quietly on startup (default on)</label>
    <label><input type="checkbox" checked={startWithOs} disabled={!loaded} onchange={(event) => void toggleAutostart(event.currentTarget.checked)} /> Start with the operating system (default off)</label>
    <p class="hint">Clipboard contents are not read while the clipboard prompt is disabled.</p>
  </section>

  <section class="settings-card">
    <h3>Pencil Download Manager</h3>
    <p>Version {appVersion}</p>
    <button type="button" disabled={busy} onclick={oncheckupdate}>Check for updates</button>
    <button type="button" onclick={() => void tauriApi.openThirdPartyNotices().catch((error) => store.say(String(error), true))}>Open THIRD_PARTY_NOTICES</button>
    <button type="button" disabled={busy} onclick={() => void copyDiagnostics()}>{busy ? "Preparing report…" : "Copy diagnostics report"}</button>
    <p class="hint">The report includes the app version, operating system, engine status, and up to 200 sanitized setup-log lines. URLs, paths, and file names are excluded.</p>
  </section>

  <section class="settings-card" aria-labelledby="engine-about-title">
    <h3 id="engine-about-title">Download engine: aria2</h3>
    {#if aria2}
      <p>aria2 {aria2.version} (upstream tag <code>{aria2.tag}</code>), licensed under {aria2.license}.</p>
      <button type="button" onclick={() => void tauriApi.openAria2Source().catch((error) => store.say(String(error), true))}>Open aria2 source code</button>
      <p class="hint">Windows and macOS installers include this exact version; on Linux the system aria2 package is used. aria2 runs as a separate program that this app talks to locally.</p>
    {:else}
      <p>aria2 version information is unavailable.</p>
    {/if}
    <p class="hint">Not affiliated with the aria2 project.</p>
  </section>

  <section class="settings-card" aria-labelledby="data-title">
    <h3 id="data-title">App data</h3>
    <p class="hint">Deletes history, settings, schedules and any engine the app downloaded, then closes the app. Files you downloaded are not deleted.</p>
    {#if confirmDelete}
      <p><strong>This cannot be undone.</strong></p>
      <div class="confirm-row">
        <button type="button" disabled={busy} onclick={() => void deleteAllData()}>Yes, delete app data and quit</button>
        <button type="button" disabled={busy} onclick={() => (confirmDelete = false)}>Cancel</button>
      </div>
    {:else}
      <button type="button" disabled={busy} onclick={() => (confirmDelete = true)}>Delete all app data…</button>
    {/if}
  </section>
</section>

<style>
  .desktop-page { padding: 24px; overflow: auto; height: 100%; box-sizing: border-box; }
  .page-heading { display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-bottom: 20px; }
  h2, h3, p { margin-top: 0; }
  .page-heading p, .hint { color: var(--muted, #777); font-size: 0.9rem; }
  .settings-card { display: grid; gap: 14px; padding: 18px; margin-bottom: 18px; border: 1px solid var(--border, #ddd); border-radius: 10px; }
  .settings-card label { display: flex; align-items: center; gap: 10px; }
  .settings-card button, .page-heading button { justify-self: start; }
  .confirm-row { display: flex; gap: 10px; flex-wrap: wrap; }
</style>
