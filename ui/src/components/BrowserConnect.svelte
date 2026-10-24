<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  let code = $state("");
  let error = $state("");
  let loading = $state(false);

  async function createCode() {
    loading = true;
    error = "";
    try { code = await invoke<string>("create_browser_pairing_code"); }
    catch (e) { error = String(e); }
    finally { loading = false; }
  }
</script>

<section class="browser-connect" aria-labelledby="browser-title">
  <div class="browser-copy">
    <h2 id="browser-title">Browser integration</h2>
    <p>Pair the Pencil extension to send browser downloads to this app.</p>
  </div>
  {#if code}
    <code class="pairing-code" aria-label="One-time pairing code">{code}</code>
    <button type="button" onclick={() => (code = "")}>Hide code</button>
  {:else}
    <button class="primary" type="button" disabled={loading} onclick={createCode}>
      {loading ? "Generating…" : "Connect browser"}
    </button>
  {/if}
  {#if error}<p class="browser-error" role="alert">{error}</p>{/if}
</section>

<style>
  .browser-connect { margin: 10px 14px 0; padding: 10px 12px; border: 1px solid var(--border); border-radius: 10px; display: flex; align-items: center; gap: 12px; flex-wrap: wrap; background: var(--surface); }
  .browser-copy { flex: 1; min-width: 180px; }
  h2 { margin: 0; font-size: 13px; }
  p { margin: 2px 0 0; color: var(--muted); font-size: 12px; }
  .pairing-code { padding: 5px 10px; border-radius: 6px; background: var(--surface-2); font-size: 18px; font-weight: 700; letter-spacing: .12em; user-select: all; }
  .browser-error { width: 100%; color: var(--danger); }
</style>
