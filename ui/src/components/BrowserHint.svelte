<script lang="ts">
  import { onMount } from "svelte";
  import { tauriApi } from "../lib/api";

  let { paired, onsetup, onpaired }: { paired: boolean; onsetup: () => void; onpaired: (paired: boolean) => void } = $props();
  let dismissed = $state(true);
  let loaded = $state(false);

  onMount(() => {
    void (async () => {
      try {
        const [status, settings] = await Promise.all([tauriApi.getBrowserStatus(), tauriApi.getSettings()]);
        onpaired(status.paired);
        dismissed = Object.fromEntries(settings).browser_hint_dismissed === "true";
      } catch {
        dismissed = true;
      } finally {
        loaded = true;
      }
    })();
  });

  async function dismiss() {
    dismissed = true;
    try { await tauriApi.setSetting("browser_hint_dismissed", "true"); } catch { /* the hint just returns next launch */ }
  }
</script>

{#if loaded && !paired && !dismissed}
  <aside class="hint" role="note">
    <span>Want browser downloads to land here? Add the Pencil extension in about a minute.</span>
    <button type="button" class="primary" onclick={onsetup}>Set up browser</button>
    <button type="button" onclick={dismiss}>Not now</button>
  </aside>
{/if}

<style>
  .hint { margin: 10px 18px 0; padding: 8px 8px 8px 16px; display: flex; align-items: center; gap: 10px; flex-wrap: wrap; border: 1px solid var(--border); border-radius: 999px; background: color-mix(in srgb, var(--surface) 80%, transparent); }
  .hint span { flex: 1; min-width: 200px; color: var(--muted); }
</style>
