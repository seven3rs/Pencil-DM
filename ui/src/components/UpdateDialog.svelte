<script lang="ts">
  import type { Update } from "@tauri-apps/plugin-updater";

  let {
    update,
    busy = false,
    error = "",
    oninstall,
    onclose,
  }: {
    update: Update;
    busy?: boolean;
    error?: string;
    oninstall: () => void;
    onclose: () => void;
  } = $props();
</script>

<div class="overlay" role="presentation">
  <section class="dialog" role="dialog" aria-modal="true" aria-labelledby="update-title">
    <h2 id="update-title">Pencil Download Manager {update.version} is available</h2>
    <h3>What's new</h3>
    {#if update.body?.trim()}
      <div class="changelog">{update.body}</div>
    {:else}
      <p class="muted">No changelog was provided for this release.</p>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <p class="muted">The app will restart after the signed update has been installed.</p>
    <footer>
      <button type="button" disabled={busy} onclick={onclose}>Later</button>
      <button type="button" class="primary" disabled={busy} onclick={oninstall}>{busy ? "Installing update…" : "Restart to update"}</button>
    </footer>
  </section>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 1000; display: grid; place-items: center; padding: 20px; background: rgb(0 0 0 / 45%); }
  .dialog { width: min(560px, 100%); max-height: 80vh; overflow: auto; padding: 24px; border: 1px solid var(--border, #ddd); border-radius: 12px; background: var(--panel, var(--surface, white)); color: var(--text, inherit); box-shadow: 0 18px 60px rgb(0 0 0 / 25%); }
  h2 { margin-top: 0; }
  .muted { color: var(--muted, #777); font-size: .92rem; }
  .changelog { max-height: 32vh; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; padding: 12px; border-radius: 8px; background: var(--background, rgb(127 127 127 / 10%)); }
  .error { color: var(--danger, #b42318); }
  footer { display: flex; justify-content: flex-end; gap: 10px; margin-top: 20px; }
  button { padding: 8px 12px; }
  .primary { font-weight: 600; }
</style>
