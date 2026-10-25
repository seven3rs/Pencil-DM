<script lang="ts">
  import { onMount } from "svelte";
  import type { DownloadStore } from "../lib/store.svelte";
  import { tauriApi } from "../lib/api";

  let { store, onadd, onscheduler, onabout, onbrowser, browserPaired = false }: { store: DownloadStore; onadd: () => void; onscheduler: () => void; onabout: () => void; onbrowser: () => void; browserPaired?: boolean } = $props();
  let limiterEnabled = $state(false);
  let limitKib = $state(1024);
  let settingsLoaded = $state(false);
  let lastApplied = $state("");

  const label = $derived(
    store.engine.state === "ready"
      ? "Ready"
      : store.engine.state === "error"
        ? "Engine problem"
        : store.engine.state === "missing"
          ? "Setup needed"
          : "Starting…",
  );

  onMount(() => {
    void (async () => {
      try {
        const settings = Object.fromEntries(await tauriApi.getSettings());
        limiterEnabled = settings.speed_limiter_enabled === "true";
        limitKib = Math.max(1, Math.round(Number(settings.speed_limiter_bytes_per_second || 1048576) / 1024));
      } catch (error) {
        store.say(String(error), true);
      } finally {
        settingsLoaded = true;
      }
    })();
  });

  $effect(() => {
    if (!settingsLoaded || !store.ready) return;
    const key = `${limiterEnabled}:${limitKib}`;
    if (key === lastApplied) return;
    lastApplied = key;
    void tauriApi.setSpeedLimiter(limiterEnabled, Math.round(limitKib * 1024))
      .catch((error) => store.say(String(error), true));
  });
</script>

<header class="topbar">
  <div class="brand">
    <img class="logo" src="/logo.svg" alt="" width="32" height="32" />
    <h1><b>Pencil</b> <span>Download Manager</span></h1>
  </div>
  <span class="badge {store.engine.state}" role="status">{label}</span>
  <label class="speed-toggle">
    <input type="checkbox" bind:checked={limiterEnabled} disabled={!store.ready} />
    Speed limiter
  </label>
  <label class="speed-value">
    KiB/s
    <input type="number" min="1" max="1048576" bind:value={limitKib} disabled={!store.ready} />
  </label>
  <button type="button" onclick={onbrowser} title={browserPaired ? "Browser extension connected" : "Connect your browser"}>
    Browser{#if browserPaired}<span class="paired-dot" aria-label="connected"></span>{/if}
  </button>
  <button type="button" onclick={onscheduler}>Scheduler</button>
  <button type="button" onclick={onabout}>About</button>
  <button type="button" class="primary" onclick={onadd} disabled={!store.ready} title="Add URL (Ctrl/Cmd+N)">
    Add URL
  </button>
</header>
