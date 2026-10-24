<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
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

  const store = new DownloadStore(tauriApi);
  let addOpen = $state(false);
  let menuOpen = $state(false);
  let schedulerOpen = $state(false);
  let powerCountdown = $state<{action: string; seconds: number} | null>(null);
  let powerCleanup: Array<() => void> = [];

  onMount(() => {
    void store.start();
    void Promise.all([
      listen<{action: string; seconds: number}>("power-countdown", (event) => powerCountdown = event.payload),
      listen("power-countdown-cancelled", () => powerCountdown = null),
    ]).then(cleanups => powerCleanup.push(...cleanups));
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

<svelte:window onkeydown={onKeydown} />

<div class="shell">
  <TopBar {store} onadd={() => (addOpen = true)} onscheduler={() => (schedulerOpen = !schedulerOpen)} />
  <Sidebar {store} />

  <div class="main">
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

<AddDialog {store} open={addOpen} onclose={() => (addOpen = false)} />
