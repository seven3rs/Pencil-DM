<script lang="ts">
  import { formatBytes, formatEta, formatPercent, formatSpeed, statusLabel } from "../lib/format";
  import type { Download } from "../lib/types";

  let { download, onclose }: { download: Download; onclose: () => void } = $props();
</script>

<!-- Every value below comes from the network or the engine: it is always rendered as text. -->
<aside class="details" aria-label="Download details">
  <header>
    <h2>{download.name}</h2>
    <button type="button" onclick={onclose} aria-label="Close details">✕</button>
  </header>
  <dl>
    <dt>Status</dt>
    <dd>{statusLabel(download.status)}</dd>
    <dt>Progress</dt>
    <dd>
      {#if download.percent !== null}
        <progress max="100" value={download.percent} aria-label="Download progress"></progress>
      {/if}
      {formatPercent(download.percent)}
    </dd>
    <dt>Speed</dt>
    <dd>{formatSpeed(download.smoothSpeed)}</dd>
    <dt>ETA</dt>
    <dd>{formatEta(download.eta)}</dd>
    <dt>Size</dt>
    <dd>{formatBytes(download.completed)} of {formatBytes(download.total)}</dd>
    <dt>Path</dt>
    <dd>{download.path || "—"}</dd>
    {#if download.error}
      <dt>Error</dt>
      <dd class="err">{download.error}</dd>
    {/if}
  </dl>
</aside>
