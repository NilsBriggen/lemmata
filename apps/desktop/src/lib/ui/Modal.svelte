<script lang="ts">
  // The one dialog frame: backdrop, title, body, footer. Esc and clicking
  // the backdrop close it; focus stays inside while it's open.
  import { onMount, type Snippet } from "svelte";
  import X from "@lucide/svelte/icons/x";

  interface Props {
    title: string;
    subtitle?: string;
    width?: number;
    /** Top-aligned for search-like dialogs, centred otherwise. */
    top?: boolean;
    onClose: () => void;
    children: Snippet;
    footer?: Snippet;
  }
  let { title, subtitle, width = 520, top = false, onClose, children, footer }: Props = $props();

  let panel = $state<HTMLDivElement>();
  let opener: Element | null = null;

  onMount(() => {
    opener = document.activeElement;
    const first = panel?.querySelector<HTMLElement>("[autofocus], input, select, textarea, button:not(.close)");
    first?.focus();
    return () => (opener as HTMLElement | null)?.focus?.();
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    } else if (e.key === "Tab" && panel) {
      const items = [...panel.querySelectorAll<HTMLElement>("button, input, select, textarea, a[href], [tabindex]:not([tabindex='-1'])")].filter((el) => !el.hasAttribute("disabled"));
      if (!items.length) return;
      const [first, last] = [items[0], items[items.length - 1]];
      if (e.shiftKey && document.activeElement === first) (e.preventDefault(), last.focus());
      else if (!e.shiftKey && document.activeElement === last) (e.preventDefault(), first.focus());
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" class:top onmousedown={(e) => e.target === e.currentTarget && onClose()} onkeydown={onKey}>
  <div class="panel" bind:this={panel} role="dialog" aria-modal="true" aria-label={title} style:width="min({width}px, 100%)">
    <header>
      <div>
        <h2>{title}</h2>
        {#if subtitle}<p>{subtitle}</p>{/if}
      </div>
      <button class="btn ghost icon sm close" aria-label="Close" onclick={onClose}><X size={16} /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 50; display: grid; place-items: center; padding: 24px;
    background: rgba(12, 17, 22, 0.42); backdrop-filter: blur(2px); animation: fade-in 0.12s ease;
  }
  .backdrop.top { place-items: start center; padding-top: 11vh; }
  .panel {
    max-height: calc(100vh - 48px); display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--border); border-radius: 12px;
    box-shadow: var(--shadow-lg); animation: pop 0.14s var(--ease); overflow: hidden;
  }
  header { display: flex; align-items: flex-start; gap: 12px; padding: 18px 20px 4px; }
  header > div { flex: 1; min-width: 0; }
  h2 { font: 600 16px var(--f-body); margin: 0; color: var(--ink); }
  header p { margin: 3px 0 0; color: var(--muted); }
  .body { padding: 12px 20px 18px; overflow: auto; display: flex; flex-direction: column; gap: 14px; }
  footer {
    display: flex; justify-content: flex-end; align-items: center; gap: 8px;
    padding: 12px 20px; border-top: 1px solid var(--border); background: var(--surface-2);
  }
</style>
