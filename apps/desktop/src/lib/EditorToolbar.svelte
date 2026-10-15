<script lang="ts">
  // Formatting and insert buttons above the editor, like Overleaf's.
  import MessageSquarePlus from "@lucide/svelte/icons/message-square-plus";
  import Bold from "@lucide/svelte/icons/bold";
  import Italic from "@lucide/svelte/icons/italic";
  import Heading from "@lucide/svelte/icons/heading";
  import Sigma from "@lucide/svelte/icons/sigma";
  import Pi from "@lucide/svelte/icons/pi";
  import Link from "@lucide/svelte/icons/link";
  import ImageIcon from "@lucide/svelte/icons/image";
  import Table from "@lucide/svelte/icons/table";
  import Quote from "@lucide/svelte/icons/quote";
  import List from "@lucide/svelte/icons/list";
  import ListOrdered from "@lucide/svelte/icons/list-ordered";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Locate from "@lucide/svelte/icons/locate";
  import DropMenu from "./ui/DropMenu.svelte";
  import { display, keysOf } from "./shortcuts";

  interface Props {
    disabled: boolean;
    onBold: () => void;
    onItalic: () => void;
    onHeading: (command: string) => void;
    onMath: () => void;
    onSymbols: () => void;
    onLink: () => void;
    onFigure: () => void;
    onTable: () => void;
    onCite: () => void;
    onList: (ordered: boolean) => void;
    /** Shows the line under the cursor in the PDF. The button is hidden without it. */
    onSyncToPdf?: () => void;
    /** Starts a review comment on the selection. */
    onComment?: () => void;
  }
  let p: Props = $props();

  let headingMenu = $state(false);
  let anchor = $state<HTMLElement>();
  const headings = [
    ["section", "Section"],
    ["subsection", "Subsection"],
    ["subsubsection", "Subsubsection"],
    ["paragraph", "Paragraph"],
    ["chapter", "Chapter"],
  ];
  const tip = (label: string, id?: string) => (id ? `${label} (${display(keysOf(id))})` : label);
</script>

<div class="bar" role="toolbar" aria-label="Formatting">
  <div class="group">
    <button class="btn ghost icon sm" title={tip("Bold", "bold")} aria-label="Bold" disabled={p.disabled} onclick={p.onBold}><Bold size={16} /></button>
    <button class="btn ghost icon sm" title={tip("Italic", "italic")} aria-label="Italic" disabled={p.disabled} onclick={p.onItalic}><Italic size={16} /></button>
    <button bind:this={anchor} class="btn ghost sm heading" title="Heading" aria-haspopup="menu" aria-expanded={headingMenu} disabled={p.disabled} onclick={() => (headingMenu = !headingMenu)}>
      <Heading size={16} /> <ChevronDown size={12} />
    </button>
    {#if headingMenu && anchor}
      <DropMenu {anchor} onClose={() => (headingMenu = false)}>
        {#each headings as [cmd, label] (cmd)}
          <button role="menuitem" onclick={() => ((headingMenu = false), p.onHeading(cmd))}>{label}</button>
        {/each}
      </DropMenu>
    {/if}
  </div>
  <span class="sep"></span>
  <div class="group">
    <button class="btn ghost icon sm" title="Inline maths" aria-label="Inline maths" disabled={p.disabled} onclick={p.onMath}><Sigma size={16} /></button>
    <button class="btn ghost icon sm" title={tip("Symbols", "symbols")} aria-label="Symbols" disabled={p.disabled} onclick={p.onSymbols}><Pi size={16} /></button>
    <button class="btn ghost icon sm" title="Link" aria-label="Link" disabled={p.disabled} onclick={p.onLink}><Link size={16} /></button>
    <button class="btn ghost icon sm" title="Figure" aria-label="Figure" disabled={p.disabled} onclick={p.onFigure}><ImageIcon size={16} /></button>
    <button class="btn ghost icon sm" title="Table" aria-label="Table" disabled={p.disabled} onclick={p.onTable}><Table size={16} /></button>
    <button class="btn ghost icon sm" title={tip("Citation", "cite")} aria-label="Citation" disabled={p.disabled} onclick={p.onCite}><Quote size={16} /></button>
  </div>
  <span class="sep"></span>
  <div class="group">
    <button class="btn ghost icon sm" title="Bulleted list" aria-label="Bulleted list" disabled={p.disabled} onclick={() => p.onList(false)}><List size={16} /></button>
    <button class="btn ghost icon sm" title="Numbered list" aria-label="Numbered list" disabled={p.disabled} onclick={() => p.onList(true)}><ListOrdered size={16} /></button>
  </div>
  {#if p.onSyncToPdf}
    <span class="sep"></span>
    <div class="group">
      {#if p.onComment}<button class="btn ghost sm" title={tip("Add a comment on the selection", "comment")} disabled={p.disabled} onclick={p.onComment}><MessageSquarePlus size={16} /> Comment</button>{/if}
      <button class="btn ghost sm" title={tip("Go to PDF", "sync")} disabled={p.disabled} onclick={p.onSyncToPdf}><Locate size={16} /> Go to PDF</button>
    </div>
  {/if}
</div>

<style>
  .bar { display: flex; align-items: center; gap: 4px; padding: 4px 10px; background: var(--surface); border-bottom: 1px solid var(--border); flex: none; overflow-x: auto; scrollbar-width: none; }
  .group { display: flex; gap: 2px; }
  .sep { width: 1px; height: 18px; background: var(--border); margin: 0 6px; flex: none; }
  .heading { gap: 2px; }
</style>
