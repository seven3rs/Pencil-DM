<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { tauriApi } from "./lib/api";
  import { DownloadStore } from "./lib/store.svelte";
  import AddDialog from "./components/AddDialog.svelte";
  import DetailsPanel from "./components/DetailsPanel.svelte";
  import DownloadTable from "./components/DownloadTable.svelte";
  import EnginePanel from "./components/EnginePanel.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import TopBar from "./components/TopBar.svelte";

  const store = new DownloadStore(tauriApi);
  let addOpen = $state(false);
  let menuOpen = $state(false);

  onMount(() => void store.start());
  onDestroy(() => store.stop());

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
  <TopBar {store} onadd={() => (addOpen = true)} />
  <Sidebar {store} />

  <div class="main">
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

    <DownloadTable {store} onmenuchange={(open) => (menuOpen = open)} />
  </div>

  {#if store.selected}
    <DetailsPanel download={store.selected} onclose={() => store.select(null)} />
  {/if}
</div>

<AddDialog {store} open={addOpen} onclose={() => (addOpen = false)} />
