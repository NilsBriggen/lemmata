<script lang="ts">
  // Errors, warnings and layout notes from the last build, as cards; plus the
  // full TeX log. Shown in place of the PDF, like Overleaf's logs view.
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Info from "@lucide/svelte/icons/info";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import { api, errorMessage, type Diagnostic } from "./api";
  import CommandBox from "./CommandBox.svelte";
  import Excerpt from "./Excerpt.svelte";

  interface Props {
    diagnostics: Diagnostic[];
    /** Changes after every build, so the raw log reloads. */
    buildId: number;
    onGoto: (file: string, line: number) => void;
    /** Adds \usepackage{…} to the document, for "undefined command" fixes. */
    onAddPackage: (pkg: string) => void;
    /** Source text of a project file, for the excerpts. */
    getSource: (file: string) => Promise<string | null>;
  }
  let { diagnostics, buildId, onGoto, onAddPackage, getSource }: Props = $props();

  let tab = $state<"problems" | "log">("problems");
  let showInfo = $state(false);
  let log = $state<string | null>(null);
  let logError = $state<string | null>(null);

  const errors = $derived(diagnostics.filter((d) => d.severity === "error"));
  const warnings = $derived(diagnostics.filter((d) => d.severity === "warning"));
  const infos = $derived(diagnostics.filter((d) => d.severity === "info"));
  const shown = $derived([...errors, ...warnings, ...(showInfo ? infos : [])]);

  $effect(() => {
    void buildId;
    if (tab !== "log") return;
    api.readLog().then((t) => ((log = t), (logError = null))).catch((e) => (logError = errorMessage(e)));
  });

  const file = (d: Diagnostic) => d.file?.replace(/^\.\//, "") ?? null;
</script>

<div class="logs">
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === "problems"} class:on={tab === "problems"} onclick={() => (tab = "problems")}>
      Problems
      {#if errors.length}<span class="badge error">{errors.length}</span>{/if}
      {#if warnings.length}<span class="badge warn">{warnings.length}</span>{/if}
    </button>
    <button role="tab" aria-selected={tab === "log"} class:on={tab === "log"} onclick={() => (tab = "log")}>Raw log</button>
  </div>

  {#if tab === "problems"}
    <div class="list">
      {#if !errors.length && !warnings.length}
        <div class="clear">
          <CircleCheck size={28} />
          <p><b>No errors or warnings.</b></p>
          {#if infos.length}<p>{infos.length} layout note{infos.length === 1 ? "" : "s"} about overfull or underfull lines below.</p>{/if}
        </div>
      {/if}
      {#each shown as d, i (i)}
        <article class="card {d.severity}">
          <header>
            <span class="ico">
              {#if d.severity === "error"}<CircleAlert size={16} />{:else if d.severity === "warning"}<TriangleAlert size={16} />{:else}<Info size={16} />{/if}
            </span>
            <span class="msg">{d.message}</span>
            {#if file(d)}
              <button class="loc" disabled={!d.line} onclick={() => d.line && onGoto(file(d)!, d.line)} title="Go to this line">
                {file(d)}{d.line ? `:${d.line}` : ""}
              </button>
            {/if}
          </header>
          {#if d.severity === "error" && file(d) && d.line && i < 12}
            <Excerpt file={file(d)!} line={d.line} {getSource} />
          {/if}
          {#if d.fix}
            <p class="hint">{d.fix.explanation}</p>
            {#if d.fix.package}
              <div class="action"><button class="btn sm" onclick={() => onAddPackage(d.fix!.package!)}>Add \usepackage{"{"}{d.fix.package}{"}"}</button></div>
            {/if}
            {#if d.fix.command && d.kind.type === "missing-file"}
              <CommandBox command={d.fix.command} install={{ what: "package", file: d.kind.name }} />
              <p class="after">After installing, recompile.</p>
            {/if}
          {:else if d.hint}
            <p class="hint">{d.hint}</p>
          {/if}
        </article>
      {/each}
      {#if infos.length}
        <label class="check toggle"><input type="checkbox" bind:checked={showInfo} /> Show {infos.length} layout note{infos.length === 1 ? "" : "s"}</label>
      {/if}
    </div>
  {:else}
    <div class="raw">
      {#if logError}<p class="muted">{logError}</p>{:else if log === null}<p class="muted">Loading…</p>{:else}<pre>{log}</pre>{/if}
    </div>
  {/if}
</div>

<style>
  .logs { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--surface-2); }
  .tabs { display: flex; gap: 2px; padding: 8px 12px 0; border-bottom: 1px solid var(--border); background: var(--surface); flex: none; }
  .tabs button {
    display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px;
    border: none; background: none; border-bottom: 2px solid transparent; margin-bottom: -1px;
    font: 500 13px var(--f-body); color: var(--muted);
  }
  .tabs button.on { color: var(--ink); border-bottom-color: var(--accent); }
  .list { overflow: auto; padding: 12px; display: flex; flex-direction: column; gap: 8px; flex: 1; }
  .card {
    background: var(--surface); border: 1px solid var(--border); border-left: 3px solid var(--faint);
    border-radius: var(--r); padding: 10px 12px; display: flex; flex-direction: column; gap: 6px;
  }
  .card.error { border-left-color: var(--error); }
  .card.warning { border-left-color: var(--warn); }
  .card header { display: flex; align-items: flex-start; gap: 8px; }
  .ico { display: inline-flex; padding-top: 1px; color: var(--faint); }
  .error .ico { color: var(--error); }
  .warning .ico { color: var(--warn); }
  .msg { flex: 1; min-width: 0; font-weight: 500; color: var(--ink); overflow-wrap: anywhere; }
  .loc {
    flex: none; border: 1px solid var(--border); background: var(--surface-2); border-radius: var(--r-sm);
    font: 11.5px var(--f-mono); color: var(--accent); padding: 1px 6px; max-width: 45%;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .loc:hover:not(:disabled) { border-color: var(--accent); }
  .loc:disabled { color: var(--muted); }
  .hint { margin: 0 0 0 24px; color: var(--muted); }
  .action { margin: 2px 0 0 24px; }
  .after { margin: 0 0 0 24px; color: var(--muted); font-size: 12px; }
  .card :global(.box) { margin-left: 24px; }
  .clear { display: flex; flex-direction: column; align-items: center; text-align: center; color: var(--ok); padding: 36px 16px 16px; gap: 2px; }
  .clear p { margin: 0; color: var(--muted); }
  .clear b { color: var(--ink); }
  .toggle { color: var(--muted); padding: 4px 2px; font-size: 12px; }
  .raw { flex: 1; overflow: auto; background: var(--surface); }
  pre { margin: 0; padding: 12px 14px; font: 12px/1.5 var(--f-mono); color: var(--ink-2); white-space: pre-wrap; overflow-wrap: anywhere; }
  .muted { color: var(--muted); padding: 12px 14px; margin: 0; }
</style>
