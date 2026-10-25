<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { tauriApi, type BrowserInfo, type BrowserStatus } from "../lib/api";
  import type { DownloadStore } from "../lib/store.svelte";
  import { installInstructions, lastSeenLabel, pairingRemaining, visibleBrowsers } from "../lib/browser";

  let { store, onclose, onstatus }: { store: DownloadStore; onclose: () => void; onstatus: (paired: boolean) => void } = $props();

  const CODE_LIFETIME_MS = 5 * 60 * 1000;

  let status = $state<BrowserStatus | null>(null);
  let browsers = $state<BrowserInfo[]>([]);
  let showAll = $state(false);
  let prepared = $state<{ path: string; family: "chromium" | "firefox"; name: string } | null>(null);
  let code = $state("");
  let codeExpires = $state(0);
  let now = $state(Date.now());
  let busy = $state("");
  let error = $state("");
  let copied = $state("");
  let timers: ReturnType<typeof setInterval>[] = [];

  const remaining = $derived(code ? pairingRemaining(codeExpires, now) : null);
  const shown = $derived(visibleBrowsers(browsers, showAll));
  const anyInstalled = $derived(browsers.some((browser) => browser.installed));

  async function refreshStatus() {
    try {
      status = await tauriApi.getBrowserStatus();
      onstatus(status.paired);
      if (status.paired) code = "";
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    void refreshStatus();
    void tauriApi.listBrowsers().then((list) => (browsers = list)).catch((e) => (error = String(e)));
    timers = [setInterval(() => void refreshStatus(), 2000), setInterval(() => (now = Date.now()), 1000)];
  });
  onDestroy(() => timers.forEach(clearInterval));

  async function run(label: string, work: () => Promise<void>) {
    busy = label;
    error = "";
    try { await work(); } catch (e) { error = String(e); } finally { busy = ""; }
  }

  const install = (browser: BrowserInfo) => run(browser.id, async () => {
    try {
      const path = await tauriApi.openBrowserExtensions(browser.id);
      prepared = { path, family: browser.family, name: browser.name };
    } catch (e) {
      // The folder is the important part; still hand it over if only opening the browser failed.
      const path = await tauriApi.prepareExtension(browser.family);
      prepared = { path, family: browser.family, name: browser.name };
      error = `${String(e)} Open the extensions page in ${browser.name} yourself and follow the steps.`;
    }
  });

  const prepare = (family: "chromium" | "firefox") => run(family, async () => {
    const path = await tauriApi.prepareExtension(family);
    prepared = { path, family, name: family === "firefox" ? "Firefox" : "your browser" };
  });

  const newCode = () => run("code", async () => {
    code = await tauriApi.createBrowserPairingCode();
    codeExpires = Date.now() + CODE_LIFETIME_MS;
    now = Date.now();
  });

  async function copy(label: string, text: string) {
    try {
      await tauriApi.writeClipboard(text);
      copied = label;
      setTimeout(() => { if (copied === label) copied = ""; }, 1600);
    } catch (e) { error = String(e); }
  }

  const reveal = () => prepared && run("reveal", () => tauriApi.revealExtensionFolder(prepared!.family));

  const disconnect = () => {
    if (!confirm("Disconnect the browser extension? Browser downloads will stay in the browser until you pair it again.")) return;
    return run("disconnect", async () => { await tauriApi.disconnectBrowser(); await refreshStatus(); });
  };
</script>

<section class="browser-page" aria-labelledby="browser-title">
  <header>
    <div>
      <h2 id="browser-title">Browser integration</h2>
      <p>Send downloads from Chrome, Edge, Brave, Vivaldi or Firefox straight to Pencil.</p>
    </div>
    <button type="button" onclick={onclose}>Back to downloads</button>
  </header>

  {#if status?.discovery_error}
    <p class="notice warn" role="alert">{status.discovery_error}</p>
  {/if}
  {#if error}<p class="notice warn" role="alert">{error}</p>{/if}

  <div class="state" class:on={status?.paired} role="status">
    <span class="dot" aria-hidden="true"></span>
    {#if status?.paired}
      <div>
        <strong>Browser connected</strong>
        <small>Extension last seen {lastSeenLabel(status.last_seen, now)}. It stays paired when you restart Pencil.</small>
      </div>
      <button type="button" class="danger" disabled={busy === "disconnect"} onclick={disconnect}>Disconnect</button>
    {:else}
      <div>
        <strong>Not connected yet</strong>
        <small>Follow the two steps below. This page updates by itself when the extension pairs.</small>
      </div>
    {/if}
  </div>

  <ol class="steps">
    <li>
      <h3>Add the extension to your browser</h3>
      <p>Pencil puts a ready-to-load copy of the extension in a folder that stays put, then opens your browser's extensions page.</p>
      <div class="choices">
        {#each shown as browser (browser.id)}
          <button type="button" class="primary" disabled={busy !== ""} onclick={() => install(browser)}>
            {busy === browser.id ? "Opening…" : `Set up ${browser.name}`}
          </button>
        {/each}
        {#if !anyInstalled && !showAll}
          <p class="muted">No supported browser was detected. Prepare the folder and load it manually.</p>
          <button type="button" disabled={busy !== ""} onclick={() => prepare("chromium")}>Prepare for Chrome, Edge or Brave</button>
          <button type="button" disabled={busy !== ""} onclick={() => prepare("firefox")}>Prepare for Firefox</button>
        {/if}
        {#if browsers.some((browser) => !browser.installed)}
          <button type="button" class="link" onclick={() => (showAll = !showAll)}>{showAll ? "Show installed only" : "Show all browsers"}</button>
        {/if}
      </div>

      {#if prepared}
        <div class="prepared" aria-live="polite">
          <p class="label">Extension folder</p>
          <div class="path-row">
            <code>{prepared.path}</code>
            <button type="button" onclick={() => copy("path", prepared!.path)}>{copied === "path" ? "Copied" : "Copy path"}</button>
            <button type="button" disabled={busy !== ""} onclick={reveal}>Open folder</button>
          </div>
          <ol class="how">
            {#each installInstructions(prepared.family) as line}<li>{line}</li>{/each}
          </ol>
          <p class="muted">After a Pencil update the folder is refreshed automatically. If the browser keeps the old version, press the reload icon on the extension card.</p>
        </div>
      {/if}
    </li>

    <li>
      <h3>Pair it once</h3>
      <p>Click the Pencil icon in your browser toolbar and enter the code below. You only do this once per browser.</p>
      {#if status?.paired}
        <p class="ok-line">Paired. Nothing more to do.</p>
      {:else if code && remaining}
        <div class="code-row">
          <code class="pairing-code" aria-label="One-time pairing code">{code}</code>
          <span class="muted">expires in {remaining}</span>
          <button type="button" onclick={() => copy("code", code)}>{copied === "code" ? "Copied" : "Copy"}</button>
          <button type="button" disabled={busy !== ""} onclick={newCode}>New code</button>
        </div>
      {:else}
        <button type="button" class="primary" disabled={busy !== ""} onclick={newCode}>
          {code ? "Code expired — get a new one" : busy === "code" ? "Generating…" : "Show pairing code"}
        </button>
      {/if}
    </li>
  </ol>
</section>

<style>
  .browser-page { overflow: auto; padding: 20px 22px 28px; display: grid; gap: 16px; align-content: start; }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; }
  h2 { margin: 0 0 4px; font-size: 20px; letter-spacing: -.01em; }
  h3 { margin: 0 0 4px; font-size: 16px; }
  p { margin: 0; color: var(--muted); }
  .notice { padding: 9px 14px; border-radius: 12px; border: 1px solid var(--danger); color: var(--danger); overflow-wrap: anywhere; }
  .state { display: flex; align-items: center; gap: 14px; padding: 14px 18px; border: 1px solid var(--border); border-radius: 16px; background: var(--surface); }
  .state > div { flex: 1; min-width: 0; }
  .state strong { display: block; }
  .state small { color: var(--muted); }
  .dot { width: 10px; height: 10px; border-radius: 50%; background: var(--muted); flex: none; }
  .state.on { border-color: var(--ok); }
  .state.on .dot { background: var(--ok); box-shadow: 0 0 0 4px color-mix(in srgb, var(--ok) 20%, transparent); }
  .steps { list-style: none; counter-reset: s; margin: 0; padding: 0; display: grid; gap: 14px; }
  .steps > li { counter-increment: s; position: relative; padding: 18px 20px 18px 66px; border: 1px solid var(--border); border-radius: 16px; background: var(--surface); }
  .steps > li::before {
    content: counter(s); position: absolute; left: 18px; top: 18px; width: 34px; height: 34px; border-radius: 50%;
    display: grid; place-items: center; font-weight: 800; color: #14161f;
    background: linear-gradient(135deg, var(--pink), var(--violet) 55%, var(--blue));
  }
  .choices { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; margin-top: 12px; }
  .choices .muted { flex-basis: 100%; }
  .muted { color: var(--muted); font-size: 13px; }
  button.link { border: 0; background: none; padding: 4px 6px; color: var(--accent-text); text-decoration: underline; text-underline-offset: 3px; }
  .prepared { margin-top: 16px; padding-top: 14px; border-top: 1px solid var(--border); display: grid; gap: 10px; }
  .label { font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: .08em; }
  .path-row { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .path-row code { flex: 1; min-width: 200px; padding: 8px 12px; border-radius: 10px; background: var(--surface-2); font-family: var(--mono); font-size: 12.5px; overflow-wrap: anywhere; user-select: all; }
  .how { margin: 0; padding-left: 20px; display: grid; gap: 6px; color: var(--text); }
  .code-row { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; margin-top: 12px; }
  .pairing-code { padding: 6px 14px; border-radius: 12px; background: var(--surface-2); font-family: var(--mono); font-size: 22px; font-weight: 500; letter-spacing: .14em; user-select: all; }
  .ok-line { margin-top: 10px; color: var(--ok); font-weight: 600; }
</style>
