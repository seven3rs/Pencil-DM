<script lang="ts">
  import type { DownloadStore } from "../lib/store.svelte";
  import type { FilterId } from "../lib/types";

  let { store }: { store: DownloadStore } = $props();

  const filters: { id: FilterId; label: string }[] = [
    { id: "all", label: "All" },
    { id: "downloading", label: "Downloading" },
    { id: "completed", label: "Completed" },
    { id: "paused", label: "Paused" },
    { id: "failed", label: "Failed" },
  ];
  // Placeholders until categories exist (see the persistence step).
  const categories = ["Compressed", "Documents", "Music", "Programs", "Video", "Other"];
</script>

<nav class="sidebar" aria-label="Downloads">
  <ul>
    {#each filters as f (f.id)}
      <li>
        <button
          type="button"
          class="side-btn"
          aria-current={store.filter === f.id}
          onclick={() => store.setFilter(f.id)}
        >
          <span>{f.label}</span>
          <span class="count">{store.counts[f.id]}</span>
        </button>
      </li>
    {/each}
  </ul>
  <div class="categories">
    <h2>Categories</h2>
    <ul>
      {#each categories as name (name)}
        <li>
          <button type="button" class="side-btn" disabled title="Coming soon">
            <span>{name}</span>
          </button>
        </li>
      {/each}
    </ul>
  </div>
</nav>
