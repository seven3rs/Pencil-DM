<script lang="ts">
  import type { DownloadStore } from "../lib/store.svelte";

  let { store, onadd }: { store: DownloadStore; onadd: () => void } = $props();

  const label = $derived(
    store.engine.state === "ready"
      ? "Ready"
      : store.engine.state === "error"
        ? "Engine problem"
        : store.engine.state === "missing"
          ? "Setup needed"
          : "Starting…",
  );
</script>

<header class="topbar">
  <div class="brand">
    <span class="logo" aria-hidden="true"></span>
    <h1>Pencil Download Manager</h1>
  </div>
  <span class="badge {store.engine.state}" role="status">{label}</span>
  <button type="button" class="primary" onclick={onadd} disabled={!store.ready} title="Add URL (Ctrl/Cmd+N)">
    Add URL
  </button>
</header>
