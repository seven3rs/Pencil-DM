<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { tauriApi } from "./lib/api";
  import { DownloadStore } from "./lib/store.svelte";
  import AddDialog from "./components/AddDialog.svelte";
  import DetailsPanel from "./components/DetailsPanel.svelte";
  import DownloadTable from "./components/DownloadTable.svelte";
  import EnginePanel from "./components/EnginePanel.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import TopBar from "./components/TopBar.svelte";
  import BrowserConnect from "./components/BrowserConnect.svelte";
  import SchedulerPage from "./components/SchedulerPage.svelte";
  import DesktopPage from "./components/DesktopPage.svelte";

  const store = new DownloadStore(tauriApi);
  let addOpen = $state(false);
  let menuOpen = $state(false);
  let schedulerOpen = $state(false);
  let desktopOpen = $state(false);
  let pendingAddText = $state("");
  let clipboardPrompt = $state("");
  let clipboardEnabled = $state(false);
  let powerCountdown = $state<{action: string; seconds: number} | null>(null);
  let powerCleanup: Array<() => void> = [];

  function openAddWithText(text: string) {
    pendingAddText = text;
    addOpen = true;
    desktopOpen = false;
    schedulerOpen = false;
  }

  function isDownloadUrl(value: string): boolean {
    try { return ["http:", "https:", "ftp:"].includes(new URL(value.trim()).protocol); }
    catch { return false; }
  }

  function extractLinks(value: string): string[] {
    return value.split(/\r?\n/).map((line) => line.trim()).filter((line) => isDownloadUrl(line));
  }

  async function drainPendingUrls(fallback?: string) {
    try {
      const pending = await tauriApi.getPendingUrls();
      const links = [...pending, ...(fallback ? [fallback] : [])].filter(isDownloadUrl);
      const unique = [...new Set(links)];
      if (unique.length) openAddWithText(unique.join("\n"));
    } catch (error) {
      if (fallback && isDownloadUrl(fallback)) openAddWithText(fallback);
      else store.say(String(error), true);
    }
  }

  function onDrop(ev: DragEvent) {
    const data = ev.dataTransfer;
    if (!data) return;
    const uriList = data.getData("text/uri-list");
    const plain = data.getData("text/plain");
    const links = extractLinks(uriList || plain);
    if (links.length) {
      ev.preventDefault();
      openAddWithText(links.join("\n"));
      return;
    }
    const files = Array.from(data.files || []);
    const textFiles = files.filter((file) => file.name.toLowerCase().endsWith(".txt") && file.size <= 1_000_000);
    if (textFiles.length) {
      ev.preventDefault();
      void Promise.all(textFiles.map((file) => file.text())).then((contents) => {
        const droppedLinks = contents.flatMap(extractLinks);
        if (droppedLinks.length) openAddWithText(droppedLinks.join("\n"));
      });
    }
  }

  onMount(() => {
    void store.start();
    void Promise.all([
      listen<{action: string; seconds: number}>("power-countdown", (event) => powerCountdown = event.payload),
      listen("power-countdown-cancelled", () => powerCountdown = null),
      listen<string>("clipboard-link-detected", (event) => {
        void tauriApi.getSettings().then((settings) => {
          const enabled = Object.fromEntries(settings).clipboard_monitor_enabled === "true";
          if (enabled && isDownloadUrl(event.payload)) clipboardPrompt = event.payload;
        }).catch(() => {});
      }),
      listen<string>("open-add-url", (event) => {
        void drainPendingUrls(event.payload);
      }),
    ]).then(cleanups => powerCleanup.push(...cleanups));
    void (async () => {
      try {
        const settings = Object.fromEntries(await tauriApi.getSettings());
        clipboardEnabled = settings.clipboard_monitor_enabled === "true";
        await drainPendingUrls();
      } catch (error) {
        store.say(String(error), true);
      }
    })();
    void getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type !== "drop") return;
      const paths = event.payload.paths;
      if (!paths.length) return;
      void tauriApi.readDroppedLinks(paths).then((links) => {
        if (links.length) openAddWithText(links.join("\n"));
      }).catch((error) => store.say(String(error), true));
    }).then(cleanup => powerCleanup.push(cleanup));
    const refreshClipboardSetting = () => {
      void drainPendingUrls();
      void tauriApi.getSettings().then((settings) => {
        clipboardEnabled = Object.fromEntries(settings).clipboard_monitor_enabled === "true";
        if (!clipboardEnabled) clipboardPrompt = "";
      }).catch(() => {});
    };
    window.addEventListener("focus", refreshClipboardSetting);
    window.addEventListener("desktop-settings-changed", refreshClipboardSetting);
    powerCleanup.push(() => window.removeEventListener("focus", refreshClipboardSetting));
    powerCleanup.push(() => window.removeEventListener("desktop-settings-changed", refreshClipboardSetting));
  });
  onDestroy(() => { store.stop(); for (const cleanup of powerCleanup) cleanup(); });
  async function cancelPower() { try { await tauriApi.cancelPowerAction(); powerCountdown = null; } catch (e) { store.say(String(e), true); } }

  /** True when the key press belongs to a form control or an open overlay. */
  function typingOrBusy(target: EventTarget | null): boolean {
    if (addOpen || menuOpen) return true;
    const el = target as HTMLElement | null;
    if (!el) return false;
    return (
      el.isContentEditable ||
      ["INPUT", "TEXTAREA", "SELECT", "BUTTON", "SUMMARY", "A"].includes(el.tagName)
    );
  }

  function onKeydown(ev: KeyboardEvent) {
    if ((ev.ctrlKey || ev.metaKey) && !ev.altKey && ev.key.toLowerCase() === "n") {
      ev.preventDefault();
      if (!addOpen) addOpen = true;
      return;
    }
    if (typingOrBusy(ev.target) || !store.selectedGid) return;
    if (ev.key === "Delete") {
      ev.preventDefault();
      void store.remove(store.selectedGid);
    } else if (ev.key === " ") {
      ev.preventDefault();
      void store.toggle(store.selectedGid);
    }
  }
</script>

<svelte:window onkeydown={onKeydown} ondragover={(event) => { if (event.dataTransfer?.types.length) event.preventDefault(); }} ondrop={onDrop} />

<div class="shell">
  <TopBar {store} onadd={() => (addOpen = true)} onscheduler={() => { schedulerOpen = !schedulerOpen; desktopOpen = false; }} onabout={() => { desktopOpen = !desktopOpen; schedulerOpen = false; }} />
  <Sidebar {store} />

  <div class="main">
    {#if clipboardPrompt}
      <div class="banner clipboard-banner" role="status"><span>Download this link?</span><button type="button" onclick={() => { const value = clipboardPrompt; clipboardPrompt = ""; openAddWithText(value); }}>Add link</button><button type="button" onclick={() => clipboardPrompt = ""}>Dismiss</button></div>
    {/if}
    {#if powerCountdown}
      <div class="banner power-banner" role="alert"><span>{powerCountdown.action} in {powerCountdown.seconds} seconds.</span><button type="button" onclick={cancelPower}>Cancel</button></div>
    {/if}
    {#if store.notice.text}
      <div class="banner" class:error={store.notice.isError} role={store.notice.isError ? "alert" : "status"}>
        <span>{store.notice.text}</span>
        {#if store.engine.state === "error"}
          <button type="button" onclick={() => store.recheck()}>Try again</button>
        {/if}
      </div>
    {:else if store.preparing}
      <div class="banner" role="status"><span>{store.preparing}</span></div>
    {/if}

    {#if store.engine.state === "missing"}
      <EnginePanel {store} />
    {/if}

    {#if schedulerOpen}
      <SchedulerPage {store} onclose={() => (schedulerOpen = false)} />
    {:else if desktopOpen}
      <DesktopPage {store} onclose={() => (desktopOpen = false)} />
    {:else}
      <BrowserConnect />
      <DownloadTable {store} onmenuchange={(open) => (menuOpen = open)} />
    {/if}
  </div>

  {#if store.selected}
    {#key store.selected.gid}
      <DetailsPanel download={store.selected} onclose={() => store.select(null)} />
    {/key}
  {/if}
</div>

<AddDialog {store} open={addOpen} initialText={pendingAddText} onclose={() => { addOpen = false; pendingAddText = ""; }} />
