<script lang="ts">
  import { ALLOCATION_MODES, type FileAllocationMode } from "../lib/allocation";
  import { parseLinks } from "../lib/links";
  import type { DownloadStore } from "../lib/store.svelte";

  let { store, open, onclose, initialText = "" }: { store: DownloadStore; open: boolean; onclose: () => void; initialText?: string } = $props();

  let dialog: HTMLDialogElement;
  let text = $state("");
  let busy = $state(false);
  /** Empty means "use the default saved in Settings". */
  let allocation = $state<"" | FileAllocationMode>("");
  const allocationHint = $derived(ALLOCATION_MODES.find((m) => m.value === allocation)?.hint ?? "Uses the disk space setting from the About page (instant reservation unless you changed it).");
  /** Engine rejections by line number, shown next to the local validation. */
  let rejected = $state<Record<number, string>>({});

  const lines = $derived(parseLinks(text));
  const valid = $derived(lines.filter((l) => !l.error && !rejected[l.line]));

  $effect(() => {
    if (open && !dialog.open) {
      rejected = {};
      text = initialText;
      allocation = "";
      dialog.showModal();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  });

  async function submit(ev: SubmitEvent) {
    ev.preventDefault();
    if (!valid.length || busy) return;
    busy = true;
    const failed: Record<number, string> = {};
    const rawLines = text.split(/\r?\n/);
    for (const link of valid) {
      const error = await store.add(link.text, allocation || undefined);
      if (error) {
        failed[link.line] = error;
      }
    }
    await store.afterAdds();
    busy = false;
    if (Object.keys(failed).length === 0) {
      text = "";
      onclose();
      return;
    }
    // Keep only the lines that did not make it, so the user can fix and retry.
    const remaining: string[] = [];
    const remap: Record<number, string> = {};
    rawLines.forEach((raw, i) => {
      const n = i + 1;
      if (failed[n] || lines.some((l) => l.line === n && l.error)) {
        remaining.push(raw);
        if (failed[n]) remap[remaining.length] = failed[n];
      }
    });
    text = remaining.join("\n");
    rejected = remap;
  }
</script>

<dialog bind:this={dialog} aria-labelledby="add-title" {onclose} oncancel={onclose}>
  <form class="dialog-body" onsubmit={submit}>
    <h2 id="add-title">Add downloads</h2>
    <p id="add-help">Paste one link per line. http, https and ftp links are supported.</p>
    <textarea
      bind:value={text}
      oninput={() => (rejected = {})}
      aria-describedby="add-help add-lines"
      spellcheck="false"
      placeholder="https://example.com/file.zip"
    ></textarea>
    <ul class="line-list" id="add-lines" aria-live="polite">
      {#each lines as l (l.line)}
        <li>
          <span class="ln">{l.line}</span>
          {#if l.error}
            <span class="bad">✕ {l.error}</span>
          {:else if rejected[l.line]}
            <span class="bad">✕ {rejected[l.line]}</span>
          {:else}
            <span class="good">✓ Ready</span>
          {/if}
        </li>
      {/each}
    </ul>
    <label class="allocation">
      <span>Disk space</span>
      <select bind:value={allocation} aria-describedby="allocation-hint">
        <option value="">Default (from Settings)</option>
        {#each ALLOCATION_MODES as mode (mode.value)}
          <option value={mode.value}>{mode.label}</option>
        {/each}
      </select>
    </label>
    <p class="hint" id="allocation-hint">{allocationHint}</p>
    <div class="dialog-actions">
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="submit" class="primary" disabled={!valid.length || busy || !store.ready}>
        {busy ? "Adding…" : valid.length === 1 ? "Add 1 download" : `Add ${valid.length} downloads`}
      </button>
    </div>
  </form>
</dialog>
