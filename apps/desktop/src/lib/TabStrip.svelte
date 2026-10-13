<script lang="ts">
  // The open files as tabs. Drag to reorder, middle-click or Ctrl+W to close,
  // right-click for Close others / to the right / all.
  import X from "@lucide/svelte/icons/x";
  import FileImage from "@lucide/svelte/icons/file-image";

  interface Props {
    tabs: string[];
    active: string | null;
    dirty: Record<string, boolean>;
    onSelect: (path: string) => void;
    onClose: (path: string) => void;
    onReorder: (tabs: string[]) => void;
  }
  let { tabs, active, dirty, onSelect, onClose, onReorder }: Props = $props();

  let strip = $state<HTMLDivElement>();
  let drag = $state<{ path: string; over: number } | null>(null);
  let menu = $state<{ path: string; x: number; y: number } | null>(null);
  let swallow = false;

  const isViewer = (p: string) => /\.(png|jpe?g|gif|webp|svg|pdf)$/i.test(p);

  function press(e: PointerEvent, path: string) {
    if (e.button !== 0 || (e.target as HTMLElement).closest(".x")) return;
    const sx = e.clientX;
    let started = false;
    const move = (ev: PointerEvent) => {
      if (!started && Math.abs(ev.clientX - sx) < 6) return;
      started = true;
      const els = [...(strip?.querySelectorAll<HTMLElement>(".tab") ?? [])];
      let over = els.findIndex((el) => ev.clientX < el.getBoundingClientRect().left + el.offsetWidth / 2);
      if (over < 0) over = els.length;
      drag = { path, over };
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (!started || !drag) return;
      const from = tabs.indexOf(drag.path);
      let to = drag.over;
      drag = null;
      swallow = true;
      setTimeout(() => (swallow = false), 60);
      if (from < 0) return;
      const next = tabs.filter((t) => t !== tabs[from]);
      if (to > from) to--;
      next.splice(to, 0, tabs[from]);
      if (next.join() !== tabs.join()) onReorder(next);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  function context(e: MouseEvent, path: string) {
    e.preventDefault();
    menu = { path, x: e.clientX, y: e.clientY };
  }

  function act(kind: "close" | "others" | "right" | "all") {
    if (!menu) return;
    const i = tabs.indexOf(menu.path);
    const victims = kind === "close" ? [menu.path] : kind === "others" ? tabs.filter((t) => t !== menu!.path) : kind === "right" ? tabs.slice(i + 1) : [...tabs];
    menu = null;
    for (const t of victims) onClose(t);
  }
</script>

<svelte:window onmousedown={(e) => menu && !(e.target as Element).closest(".tab-menu") && (menu = null)} onkeydown={(e) => menu && e.key === "Escape" && (menu = null)} />

<div class="tabs" bind:this={strip} role="tablist">
  {#each tabs as t, i (t)}
    {#if drag && drag.over === i && drag.path !== t}<span class="marker"></span>{/if}
    <div class="tab" class:on={t === active} class:moving={drag?.path === t} role="presentation" onpointerdown={(e) => press(e, t)} oncontextmenu={(e) => context(e, t)} onauxclick={(e) => e.button === 1 && (e.preventDefault(), onClose(t))}>
      <button class="name" role="tab" aria-selected={t === active} onclick={() => !swallow && onSelect(t)} title={t}>
        {#if isViewer(t)}<FileImage size={13} />{/if}
        {t.split("/").pop()}
      </button>
      <button class="x" class:dirty={dirty[t]} aria-label={dirty[t] ? `${t} has unsaved changes; close` : `Close ${t}`} onclick={() => onClose(t)}>
        <span class="dot"></span><X size={13} />
      </button>
    </div>
  {/each}
  {#if drag && drag.over === tabs.length}<span class="marker"></span>{/if}
</div>

{#if menu}
  <div class="menu tab-menu" role="menu" style:left="{menu.x}px" style:top="{menu.y}px">
    <button role="menuitem" onclick={() => act("close")}>Close</button>
    <button role="menuitem" disabled={tabs.length < 2} onclick={() => act("others")}>Close others</button>
    <button role="menuitem" disabled={tabs.indexOf(menu.path) === tabs.length - 1} onclick={() => act("right")}>Close to the right</button>
    <button role="menuitem" onclick={() => act("all")}>Close all</button>
  </div>
{/if}

<style>
  .tabs { display: flex; height: 36px; background: var(--surface-2); border-bottom: 1px solid var(--border); overflow-x: auto; flex: none; scrollbar-width: none; }
  .tab { display: flex; align-items: center; border-right: 1px solid var(--border); position: relative; max-width: 220px; }
  .tab.on { background: var(--code-bg); }
  .tab.on::after { content: ""; position: absolute; left: 0; right: 0; bottom: -1px; height: 1px; background: var(--code-bg); }
  .tab.on::before { content: ""; position: absolute; left: 0; right: 0; top: 0; height: 2px; background: var(--accent); }
  .tab button { border: none; background: none; }
  .tab .name { font: 13px var(--f-body); color: var(--muted); padding: 0 4px 0 14px; height: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tab.on .name { color: var(--ink); }
  .tab .x { width: 26px; height: 26px; margin-right: 4px; border-radius: var(--r-sm); display: grid; place-items: center; color: var(--faint); position: relative; }
  .tab .x :global(svg) { opacity: 0; grid-area: 1 / 1; }
  .tab .x .dot { grid-area: 1 / 1; width: 7px; height: 7px; border-radius: 50%; background: transparent; }
  .tab .x.dirty .dot { background: var(--accent); }
  .tab:hover .x :global(svg), .tab.on .x:not(.dirty) :global(svg), .tab .x:focus-visible :global(svg) { opacity: 1; }
  .tab:hover .x .dot, .tab .x:focus-visible .dot { opacity: 0; }
  .tab .x:hover { background: var(--surface-3); color: var(--ink); }
  .marker { width: 2px; background: var(--accent); flex: none; }
  .tab.moving { opacity: 0.5; }
  .name { display: inline-flex; align-items: center; gap: 5px; }
</style>
