<script lang="ts">
  import type { DownloadStore } from "../lib/store.svelte";

  let { store }: { store: DownloadStore } = $props();
  let path = $state("");
  let checking = $state(false);

  async function copy() {
    if (!store.engine.command) return;
    try {
      await navigator.clipboard.writeText(store.engine.command);
      store.say("Command copied.");
    } catch {
      store.say("Copy failed. Select the command and copy it manually.", true);
    }
  }

  async function recheck() {
    checking = true;
    try {
      await store.recheck();
    } finally {
      checking = false;
    }
  }
</script>

<section class="engine-needed" aria-labelledby="engine-needed-title">
  <h2 id="engine-needed-title">Download engine needed</h2>
  <p>{store.engine.detail || "Install the download engine, then check again."}</p>
  {#if store.engine.command}
    <code>{store.engine.command}</code>
  {/if}
  <div class="actions">
    {#if store.engine.command}
      <button type="button" onclick={copy}>Copy command</button>
    {/if}
    <button type="button" class="primary" onclick={recheck} disabled={checking}>Check again</button>
  </div>
  <details>
    <summary>Use my own engine path</summary>
    <label for="engine-path">Absolute path to the executable</label>
    <div class="path-row">
      <input id="engine-path" type="text" bind:value={path} spellcheck="false" placeholder="/usr/local/bin/…" />
      <button type="button" onclick={() => store.saveEnginePath(path)}>Save path</button>
    </div>
  </details>
</section>
