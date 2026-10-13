<script lang="ts">
  import type { Diagnostic } from "./api";
  import CommandBox from "./CommandBox.svelte";

  interface Props {
    diagnostics: Diagnostic[];
    showInfo: boolean;
    onGoto: (file: string, line: number) => void;
  }
  let { diagnostics, showInfo = $bindable(), onGoto }: Props = $props();

  const visible = $derived(diagnostics.filter((d) => showInfo || d.severity !== "info"));
  const infoCount = $derived(diagnostics.filter((d) => d.severity === "info").length);

  function location(d: Diagnostic) {
    if (!d.file) return "";
    const f = d.file.replace(/^\.\//, "");
    return d.line ? `${f}:${d.line}` : f;
  }
</script>

<div class="problems">
  {#if visible.length === 0}
    <p class="none">No errors or warnings.</p>
  {/if}
  {#each visible as d, i (i)}
    <button
      class="row {d.severity}"
      disabled={!d.file || !d.line}
      onclick={() => d.file && d.line && onGoto(d.file.replace(/^\.\//, ""), d.line)}
    >
      <b>{d.severity}</b>
      <span class="msg">
        {d.message}
        {#if d.fix}<span class="hint">{d.fix.explanation}</span>{:else if d.hint}<span class="hint">{d.hint}</span>{/if}
      </span>
      <span class="loc">{location(d)}</span>
    </button>
    {#if d.fix?.command && d.kind.type === "missing-file"}
      <div class="fix">
        <CommandBox command={d.fix.command} install={{ what: "package", file: d.kind.name }} />
        <p>After installing, press Recompile.</p>
      </div>
    {/if}
  {/each}
  {#if infoCount > 0}
    <label class="toggle"><input type="checkbox" bind:checked={showInfo} /> Show {infoCount} layout notes (overfull and underfull boxes)</label>
  {/if}
</div>

<style>
  .problems { overflow: auto; max-height: 100%; font-size: 12px; background: var(--sheet); }
  .none { color: var(--muted); padding: 8px 12px; margin: 0; }
  .row {
    display: flex; gap: 10px; align-items: baseline; width: 100%;
    border: none; border-bottom: 1px solid var(--rule); border-radius: 0;
    background: none; text-align: left; padding: 6px 12px; font: 12px var(--f-body); color: var(--ink);
  }
  .row:disabled { cursor: default; opacity: 1; }
  .row:hover:not(:disabled) { background: var(--blue-soft); }
  .row b { font: 500 11px var(--f-mono); min-width: 58px; }
  .error b { color: var(--mark); }
  .warning b { color: var(--warn); }
  .info b { color: var(--muted); }
  .msg { flex: 1; min-width: 0; }
  .hint { display: block; color: var(--muted); }
  .loc { font-family: var(--f-mono); color: var(--muted); white-space: nowrap; }
  .fix { padding: 4px 12px 10px 80px; border-bottom: 1px solid var(--rule); }
  .fix p { margin: 6px 0 0; color: var(--muted); }
  .toggle { display: flex; gap: 6px; align-items: center; padding: 6px 12px; color: var(--muted); }
</style>
