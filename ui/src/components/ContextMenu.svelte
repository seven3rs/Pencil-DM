<script lang="ts">
  import { onMount } from "svelte";

  export interface MenuItem {
    label: string;
    action: () => void;
    disabled?: boolean;
    danger?: boolean;
  }

  let { x, y, items, onclose }: { x: number; y: number; items: MenuItem[]; onclose: () => void } = $props();
  let el: HTMLDivElement;
  let left = $state(0);
  let top = $state(0);

  onMount(() => {
    // Keep the menu inside the window.
    const rect = el.getBoundingClientRect();
    left = Math.max(4, Math.min(x, window.innerWidth - rect.width - 4));
    top = Math.max(4, Math.min(y, window.innerHeight - rect.height - 4));
    buttons()[0]?.focus();
  });

  const buttons = () => Array.from(el.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));

  function onkeydown(ev: KeyboardEvent) {
    const list = buttons();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    if (ev.key === "Escape") {
      ev.preventDefault();
      ev.stopPropagation();
      onclose();
    } else if (ev.key === "ArrowDown") {
      ev.preventDefault();
      list[(i + 1) % list.length]?.focus();
    } else if (ev.key === "ArrowUp") {
      ev.preventDefault();
      list[(i - 1 + list.length) % list.length]?.focus();
    } else if (ev.key === "Tab") {
      ev.preventDefault();
      onclose();
    }
  }
</script>

<svelte:window onpointerdown={(ev) => { if (!el?.contains(ev.target as Node)) onclose(); }} onblur={onclose} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="menu"
  role="menu"
  tabindex="-1"
  bind:this={el}
  style:left="{left}px"
  style:top="{top}px"
  {onkeydown}
>
  {#each items as item (item.label)}
    <button
      type="button"
      role="menuitem"
      class:danger={item.danger}
      disabled={item.disabled}
      onclick={() => { item.action(); onclose(); }}
    >
      {item.label}
    </button>
  {/each}
</div>
