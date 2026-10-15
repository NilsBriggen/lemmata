<script module lang="ts">
  /** Arrow keys (wrapping), Home and End move focus between a menu's enabled buttons. */
  export function menuNav(e: KeyboardEvent, root: HTMLElement | undefined) {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
    e.preventDefault();
    const items = [...(root?.querySelectorAll<HTMLElement>("button:not(:disabled)") ?? [])];
    const i = items.indexOf(document.activeElement as HTMLElement);
    const next = e.key === "Home" ? 0 : e.key === "End" ? items.length - 1 : e.key === "ArrowDown" ? (i + 1) % items.length : i <= 0 ? items.length - 1 : i - 1;
    items[next]?.focus();
  }
</script>

<script lang="ts">
  // A menu that opens below (or above) the element that triggered it.
  import { onMount, type Snippet } from "svelte";

  interface Props {
    anchor: HTMLElement;
    align?: "start" | "end";
    onClose: () => void;
    children: Snippet;
  }
  let { anchor, align = "start", onClose, children }: Props = $props();
  let menu = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0 });

  onMount(() => {
    const r = anchor.getBoundingClientRect();
    const w = menu?.offsetWidth ?? 220;
    const h = menu?.offsetHeight ?? 200;
    const left = align === "end" ? r.right - w : r.left;
    const top = r.bottom + h + 6 > window.innerHeight ? r.top - h - 6 : r.bottom + 6;
    pos = { left: Math.max(8, Math.min(left, window.innerWidth - w - 8)), top: Math.max(8, top) };
    menu?.querySelector<HTMLElement>("button")?.focus();
    const away = (e: MouseEvent) => {
      if (!menu?.contains(e.target as Node) && !anchor.contains(e.target as Node)) onClose();
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("mousedown", away, true);
    window.addEventListener("keydown", esc, true);
    return () => {
      window.removeEventListener("mousedown", away, true);
      window.removeEventListener("keydown", esc, true);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="menu" role="menu" tabindex="-1" bind:this={menu} style:left="{pos.left}px" style:top="{pos.top}px" onkeydown={(e) => menuNav(e, menu)} onclick={(e) => (e.target as Element).closest("button") && onClose()}>
  {@render children()}
</div>
