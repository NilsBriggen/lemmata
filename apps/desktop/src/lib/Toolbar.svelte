<script lang="ts" module>
  export interface MenuAction {
    label: string;
    hint?: string;
    run: () => void;
    /** Draw a line above this item. */
    separator?: boolean;
  }
</script>

<script lang="ts">
  // The editor's top bar, after Overleaf's: menu, project, layout switch,
  // and one prominent Recompile button with its options.
  import MessageSquare from "@lucide/svelte/icons/message-square";
  import Menu from "@lucide/svelte/icons/menu";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import Columns2 from "@lucide/svelte/icons/columns-2";
  import FileCode from "@lucide/svelte/icons/file-code";
  import FileText from "@lucide/svelte/icons/file-text";
  import History from "@lucide/svelte/icons/rotate-ccw-clock";
  import Settings from "@lucide/svelte/icons/settings";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Check from "@lucide/svelte/icons/check";
  import DropMenu from "./ui/DropMenu.svelte";
  import type { Engine, Target } from "./api";
  import { display, keysOf } from "./shortcuts";

  interface Props {
    projectName: string;
    building: boolean;
    layout: "split" | "editor" | "pdf";
    sidebarOpen: boolean;
    historyOpen: boolean;
    /** The comments panel is open; `openComments` is how many threads are unresolved. */
    reviewOpen: boolean;
    openComments: number;
    onReview: () => void;
    autoCompile: boolean;
    engine: Engine;
    menu: MenuAction[];
    onLayout: (l: "split" | "editor" | "pdf") => void;
    onToggleSidebar: () => void;
    onHistory: () => void;
    onProjectSettings: () => void;
    onRecompile: () => void;
    onStop: () => void;
    onAutoCompile: (on: boolean) => void;
    draft: boolean;
    stopOnError: boolean;
    onDraft: (on: boolean) => void;
    onStopOnError: (on: boolean) => void;
    onClean: () => void;
    onEngine: (e: Engine) => void;
    onLibrary: () => void;
    /** Build presets from the project settings; the picker shows only when there are two or more. */
    targets?: Target[];
    mainFile?: string;
    onTarget?: (t: Target) => void;
  }
  let p: Props = $props();

  let menuBtn = $state<HTMLButtonElement>();
  let compileBtn = $state<HTMLButtonElement>();
  let targetBtn = $state<HTMLButtonElement>();
  let open = $state<null | "menu" | "compile" | "target">(null);
  const activeTarget = $derived((p.targets ?? []).find((t) => t.main === p.mainFile));

  const engines: [Engine, string][] = [
    ["pdflatex", "pdfLaTeX"],
    ["xelatex", "XeLaTeX"],
    ["lualatex", "LuaLaTeX"],
  ];
</script>

<header class="toolbar chrome">
  <div class="left">
    <button class="btn icon" bind:this={menuBtn} aria-label="Menu" aria-haspopup="menu" aria-expanded={open === "menu"} onclick={() => (open = open === "menu" ? null : "menu")}>
      <Menu size={18} />
    </button>
    <button class="brand" onclick={p.onLibrary} title="Back to all projects">Lemmata</button>
    <span class="sep">/</span>
    <span class="project" title={p.projectName}>{p.projectName}</span>
    {#if (p.targets?.length ?? 0) > 1}
      <button class="btn target" bind:this={targetBtn} aria-haspopup="menu" aria-expanded={open === "target"} title="What to build" onclick={() => (open = open === "target" ? null : "target")}>
        {activeTarget?.name ?? p.mainFile}<ChevronDown size={13} />
      </button>
    {/if}
  </div>

  <div class="center">
    <button class="btn icon" class:on={p.sidebarOpen} aria-pressed={p.sidebarOpen} aria-label="Show files and outline" title="Files and outline" onclick={p.onToggleSidebar}>
      <PanelLeft size={17} />
    </button>
    <div class="segmented" role="group" aria-label="Layout">
      <button aria-pressed={p.layout === "editor"} title="Editor only" onclick={() => p.onLayout("editor")}><FileCode size={15} /> Editor</button>
      <button aria-pressed={p.layout === "split"} title="Editor and PDF" onclick={() => p.onLayout("split")}><Columns2 size={15} /> Both</button>
      <button aria-pressed={p.layout === "pdf"} title="PDF only" onclick={() => p.onLayout("pdf")}><FileText size={15} /> PDF</button>
    </div>
  </div>

  <div class="right">
    <button class="btn" class:on={p.reviewOpen} aria-pressed={p.reviewOpen} onclick={p.onReview} title="Review comments"><MessageSquare size={16} /> Comments{#if p.openComments}<span class="pill">{p.openComments}</span>{/if}</button>
    <button class="btn" class:on={p.historyOpen} aria-pressed={p.historyOpen} onclick={p.onHistory}><History size={16} /> History</button>
    <button class="btn icon" aria-label="Project settings" title="Project settings" onclick={p.onProjectSettings}><Settings size={17} /></button>
    <div class="compile">
      <button class="btn primary main" onclick={p.building ? p.onStop : p.onRecompile} title={p.building ? "Stop compiling" : `Recompile (${display(keysOf("recompile"))})`}>
        <span class:spin={p.building}><RefreshCw size={15} /></span>
        <!-- While building, the button stops the build, so it says so. -->
        {p.building ? "Stop" : "Recompile"}
      </button>
      <button class="btn primary caret" bind:this={compileBtn} aria-label="Compile options" aria-haspopup="menu" aria-expanded={open === "compile"} onclick={() => (open = open === "compile" ? null : "compile")}>
        <ChevronDown size={15} />
      </button>
    </div>
  </div>
</header>

{#if open === "menu" && menuBtn}
  <DropMenu anchor={menuBtn} onClose={() => (open = null)}>
    {#each p.menu as item (item.label)}
      {#if item.separator}<hr />{/if}
      <button role="menuitem" onclick={item.run}>{item.label}{#if item.hint}<span class="hint">{item.hint}</span>{/if}</button>
    {/each}
  </DropMenu>
{/if}

{#if open === "target" && targetBtn}
  <DropMenu anchor={targetBtn} onClose={() => (open = null)}>
    <div class="heading">Build</div>
    {#each p.targets ?? [] as t (t.name)}
      <button role="menuitemradio" aria-checked={t.main === p.mainFile} onclick={() => ((open = null), p.onTarget?.(t))}>
        <span class="tick">{#if t.main === p.mainFile}<Check size={14} />{/if}</span> {t.name}<span class="hint">{t.main}</span>
      </button>
    {/each}
  </DropMenu>
{/if}

{#if open === "compile" && compileBtn}
  <DropMenu anchor={compileBtn} align="end" onClose={() => (open = null)}>
    <div class="heading">Compile</div>
    <button role="menuitemradio" aria-checked={p.autoCompile} onclick={() => p.onAutoCompile(true)}>
      <span class="tick">{#if p.autoCompile}<Check size={14} />{/if}</span> After every save
    </button>
    <button role="menuitemradio" aria-checked={!p.autoCompile} onclick={() => p.onAutoCompile(false)}>
      <span class="tick">{#if !p.autoCompile}<Check size={14} />{/if}</span> Only when I recompile
    </button>
    <hr />
    <div class="heading">Compiler</div>
    {#each engines as [id, label] (id)}
      <button role="menuitemradio" aria-checked={p.engine === id} onclick={() => p.onEngine(id)}>
        <span class="tick">{#if p.engine === id}<Check size={14} />{/if}</span> {label}
      </button>
    {/each}
    <hr />
    <div class="heading">Mode</div>
    <button role="menuitemradio" aria-checked={!p.draft} onclick={() => p.onDraft(false)}>
      <span class="tick">{#if !p.draft}<Check size={14} />{/if}</span> Normal
    </button>
    <button role="menuitemradio" aria-checked={p.draft} onclick={() => p.onDraft(true)} title="Figures are drawn as empty boxes, so builds are quicker">
      <span class="tick">{#if p.draft}<Check size={14} />{/if}</span> Fast (draft)
    </button>
    <hr />
    <div class="heading">When there's an error</div>
    <button role="menuitemradio" aria-checked={!p.stopOnError} onclick={() => p.onStopOnError(false)}>
      <span class="tick">{#if !p.stopOnError}<Check size={14} />{/if}</span> Keep going
    </button>
    <button role="menuitemradio" aria-checked={p.stopOnError} onclick={() => p.onStopOnError(true)}>
      <span class="tick">{#if p.stopOnError}<Check size={14} />{/if}</span> Stop at the first error
    </button>
    <hr />
    <button role="menuitem" onclick={p.onClean}>Clear cached files</button>
    {#if p.building}
      <button role="menuitem" onclick={p.onStop}>Stop compiling</button>
    {/if}
  </DropMenu>
{/if}

<style>
  .target { height: 28px; padding: 0 8px; margin-left: 8px; gap: 4px; font-size: 12.5px; }
  .toolbar {
    display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; gap: 12px;
    height: 46px; padding: 0 10px; background: var(--chrome); color: var(--chrome-ink);
    border-bottom: 1px solid var(--chrome-border); flex: none;
  }
  .left, .right, .center { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .right { justify-content: flex-end; }
  .brand {
    border: none; background: none; padding: 2px 4px; border-radius: var(--r-sm);
    font: 17px var(--f-display); color: var(--chrome-ink);
  }
  .brand:hover { color: #fff; }
  .sep { color: var(--chrome-muted); }
  .project { font-weight: 500; color: var(--chrome-ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .compile { display: flex; margin-left: 4px; }
  .compile .main { border-top-right-radius: 0; border-bottom-right-radius: 0; padding: 0 14px; min-width: 128px; }
  .compile .caret { border-top-left-radius: 0; border-bottom-left-radius: 0; width: 30px; padding: 0; border-left: 1px solid rgba(255, 255, 255, 0.25); }
  .compile span { display: inline-flex; }
  .tick { width: 16px; display: inline-flex; color: var(--accent); }
  @media (max-width: 1100px) {
    .segmented button { font-size: 0; gap: 0; padding: 0 8px; }
  }
  .pill { margin-left: 4px; padding: 0 6px; border-radius: 9px; background: rgba(220, 160, 20, 0.85); color: #1b1b1b; font-size: 11px; line-height: 17px; }
</style>
