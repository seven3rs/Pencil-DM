<script lang="ts">
  import { tick } from "svelte";
  import { formatBytes, formatEta, formatPercent, formatSpeed, statusLabel } from "../lib/format";
  import { isRunning } from "../lib/downloads";
  import type { DownloadStore } from "../lib/store.svelte";
  import { tauriApi } from "../lib/api";
  import { scrollToReveal, visibleRange } from "../lib/virtual";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  const ROW_HEIGHT = 44;

  let { store, onmenuchange }: { store: DownloadStore; onmenuchange: (open: boolean) => void } = $props();

  let viewport: HTMLDivElement;
  let scrollTop = $state(0);
  let height = $state(400);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  const rows = $derived(store.visible);
  const range = $derived(visibleRange(scrollTop, height, ROW_HEIGHT, rows.length));
  const slice = $derived(rows.slice(range.start, range.end));
  const activeIndex = $derived(rows.findIndex((d) => d.gid === store.selectedGid));

  function closeMenu() {
    menu = null;
    onmenuchange(false);
    viewport?.focus();
  }

  function openMenu(gid: string, x: number, y: number) {
    store.select(gid);
    const d = store.items.find((i) => i.gid === gid);
    if (!d) return;
    menu = {
      x,
      y,
      items: [
        d.status === "paused"
          ? { label: "Resume", action: () => void store.resume(gid) }
          : { label: "Pause", action: () => void store.pause(gid), disabled: !isRunning(d.status) },
        ...(d.status === "complete" || d.status === "completed" ? [
          { label: "Open file", action: () => void tauriApi.openDownloadFile(gid).catch((error) => store.say(String(error), true)) },
          { label: "Show in folder", action: () => void tauriApi.revealDownload(gid).catch((error) => store.say(String(error), true)) },
          { label: "Copy link", action: () => void tauriApi.copyDownloadLink(gid).catch((error) => store.say(String(error), true)) },
          { label: "Delete file from disk…", action: () => { if (confirm("Delete the completed file from disk? This cannot be undone.")) void tauriApi.deleteDownloadFile(gid, true).then(() => store.say("File deleted from disk.")).catch((error) => store.say(String(error), true)); }, danger: true },
        ] : []),
        { label: "Copy file name", action: () => void navigator.clipboard?.writeText(d.name).catch(() => {}) },
        { label: "Remove", action: () => void store.remove(gid), danger: true },
      ],
    };
    onmenuchange(true);
  }

  async function move(index: number) {
    const next = rows[Math.max(0, Math.min(rows.length - 1, index))];
    if (!next) return;
    store.select(next.gid);
    const i = rows.indexOf(next);
    viewport.scrollTop = scrollToReveal(i, viewport.scrollTop, height, ROW_HEIGHT);
    await tick();
  }

  function onkeydown(ev: KeyboardEvent) {
    const page = Math.max(1, Math.floor(height / ROW_HEIGHT) - 1);
    const from = activeIndex < 0 ? -1 : activeIndex;
    switch (ev.key) {
      case "ArrowDown": ev.preventDefault(); void move(from + 1); break;
      case "ArrowUp": ev.preventDefault(); void move(from < 0 ? 0 : from - 1); break;
      case "PageDown": ev.preventDefault(); void move(from + page); break;
      case "PageUp": ev.preventDefault(); void move(from - page); break;
      case "Home": ev.preventDefault(); void move(0); break;
      case "End": ev.preventDefault(); void move(rows.length - 1); break;
      case "ContextMenu":
      case "F10": {
        if (ev.key === "F10" && !ev.shiftKey) return;
        const d = rows[activeIndex];
        if (!d) return;
        ev.preventDefault();
        const box = viewport.getBoundingClientRect();
        openMenu(d.gid, box.left + 40, box.top + (activeIndex * ROW_HEIGHT - viewport.scrollTop) + ROW_HEIGHT);
        break;
      }
    }
  }
</script>

<div class="table-wrap">
  <div class="cols thead" role="presentation" aria-hidden="true">
    <span>Name</span>
    <span class="num col-size">Size</span>
    <span>Progress</span>
    <span class="num">Speed</span>
    <span class="num col-eta">ETA</span>
    <span>Status</span>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="viewport"
    role="grid"
    tabindex="0"
    aria-label="Downloads"
    aria-rowcount={rows.length}
    aria-activedescendant={activeIndex >= 0 ? `row-${rows[activeIndex].gid}` : undefined}
    bind:this={viewport}
    bind:clientHeight={height}
    onscroll={() => (scrollTop = viewport.scrollTop)}
    {onkeydown}
  >
    {#if rows.length === 0}
      <p class="empty">
        {store.items.length ? "Nothing in this list." : "No downloads yet. Choose Add URL to start."}
      </p>
    {:else}
      <div style:height="{rows.length * ROW_HEIGHT}px" style:position="relative">
        <div style:position="absolute" style:left="0" style:right="0" style:top="{range.start * ROW_HEIGHT}px">
          {#each slice as d, i (d.gid)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              class="cols row"
              role="row"
              tabindex="-1"
              id="row-{d.gid}"
              aria-selected={d.gid === store.selectedGid}
              aria-rowindex={range.start + i + 1}
              onclick={() => store.select(d.gid)}
              oncontextmenu={(ev) => { ev.preventDefault(); openMenu(d.gid, ev.clientX, ev.clientY); }}
            >
              <span class="cell-name" role="gridcell" title={d.name}>{d.name}</span>
              <span class="num col-size" role="gridcell">{formatBytes(d.total)}</span>
              <span class="progress-cell" role="gridcell">
                <span class="bar" aria-hidden="true">
                  <span
                    class:done={d.status === "complete"}
                    class:fail={d.status === "error"}
                    style:width="{d.percent ?? 0}%"
                  ></span>
                </span>
                <span class="pct">{formatPercent(d.percent)}</span>
              </span>
              <span class="num" role="gridcell">{formatSpeed(d.smoothSpeed)}</span>
              <span class="num col-eta" role="gridcell">{formatEta(d.eta)}</span>
              <span class="status {d.status}" role="gridcell">{statusLabel(d.status)}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={closeMenu} />
{/if}
