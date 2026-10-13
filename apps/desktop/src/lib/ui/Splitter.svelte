<script lang="ts">
  // A drag handle between two panes. Reports the new size of the pane
  // before it (`side="before"`) or after it; double-click resets.
  interface Props {
    size: number;
    min?: number;
    max?: number;
    side?: "before" | "after";
    label: string;
    onResize: (size: number) => void;
    onReset?: () => void;
  }
  let { size, min = 160, max = 900, side = "before", label, onResize, onReset }: Props = $props();
  let dragging = $state(false);

  function down(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const start = size;
    dragging = true;
    const move = (ev: PointerEvent) => {
      const delta = (ev.clientX - startX) * (side === "before" ? 1 : -1);
      onResize(Math.max(min, Math.min(max, start + delta)));
    };
    const up = () => {
      dragging = false;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 60 : 16;
    const dir = side === "before" ? 1 : -1;
    if (e.key === "ArrowLeft") onResize(Math.max(min, Math.min(max, size - step * dir)));
    else if (e.key === "ArrowRight") onResize(Math.max(min, Math.min(max, size + step * dir)));
    else return;
    e.preventDefault();
  }
</script>

<!-- A focusable role="separator" is an interactive ARIA widget (resizable). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="splitter"
  class:dragging
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuenow={Math.round(size)}
  aria-valuemin={min}
  aria-valuemax={max}
  tabindex="0"
  onpointerdown={down}
  ondblclick={onReset}
  onkeydown={key}
></div>
{#if dragging}<div class="shield"></div>{/if}

<style>
  .splitter { width: 6px; margin: 0 -3px; z-index: 5; cursor: col-resize; position: relative; flex: none; }
  .splitter::after { content: ""; position: absolute; inset: 0 2px; background: transparent; transition: background 0.15s; }
  .splitter:hover::after, .splitter.dragging::after, .splitter:focus-visible::after { background: var(--accent); }
  .splitter:focus-visible { box-shadow: none; }
  /* Keeps the PDF canvas and editor from eating pointer events mid-drag. */
  .shield { position: fixed; inset: 0; z-index: 100; cursor: col-resize; }
</style>
