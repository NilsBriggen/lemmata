<script lang="ts">
  // Snapshots of the project, with a diff of any changed file against the
  // current version and a one-file restore.
  import { onDestroy } from "svelte";
  import { EditorState } from "@codemirror/state";
  import { EditorView, lineNumbers } from "@codemirror/view";
  import { unifiedMergeView } from "@codemirror/merge";
  import { api, errorMessage, type HistoryStatus, type Snapshot } from "./api";

  interface Props {
    /** Bumped by the app after it takes a snapshot, to reload the list. */
    version: number;
    /** Current contents of a file, including unsaved edits. */
    currentText: (path: string) => Promise<string | null>;
    onRestored: (path: string) => void;
    onClose: () => void;
  }
  let { version, currentText, onRestored, onClose }: Props = $props();

  let status = $state<HistoryStatus | null>(null);
  let snapshots = $state<Snapshot[]>([]);
  let openId = $state<string | null>(null);
  let diffFor = $state<{ id: string; path: string } | null>(null);
  let error = $state<string | null>(null);
  let confirmRestore = $state(false);
  let diffHost = $state<HTMLDivElement>();
  let diffView: EditorView | null = null;

  async function load() {
    error = null;
    try {
      status = await api.historyStatus();
      snapshots = status.state === "on" ? await api.historyList() : [];
    } catch (e) {
      error = errorMessage(e);
    }
  }

  $effect(() => {
    version;
    load();
  });

  async function enable() {
    try {
      await api.historyEnable();
      await load();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function showDiff(id: string, path: string) {
    diffFor = { id, path };
    confirmRestore = false;
    const [then, now] = await Promise.all([api.historyFile(id, path), currentText(path)]);
    diffView?.destroy();
    if (!diffHost) return;
    diffView = new EditorView({
      parent: diffHost,
      state: EditorState.create({
        doc: now ?? "",
        extensions: [
          lineNumbers(),
          EditorView.editable.of(false),
          EditorState.readOnly.of(true),
          EditorView.lineWrapping,
          unifiedMergeView({ original: then ?? "", mergeControls: false, collapseUnchanged: { margin: 3, minSize: 6 } }),
          EditorView.theme({
            "&": { height: "100%", fontSize: "12.5px", backgroundColor: "var(--code-bg)", color: "var(--ink)" },
            ".cm-scroller": { fontFamily: "var(--f-mono)" },
            ".cm-gutters": { backgroundColor: "var(--code-bg)", border: "none", color: "var(--muted)" },
          }),
        ],
      }),
    });
  }

  async function restore() {
    if (!diffFor) return;
    try {
      await api.historyRestore(diffFor.id, diffFor.path);
      onRestored(diffFor.path);
      confirmRestore = false;
      diffFor = null;
      await load();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  const when = (t: number) =>
    new Date(t * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

  onDestroy(() => diffView?.destroy());
</script>

<aside class="history">
  <div class="bar">
    <b>History</b>
    <button class="close" aria-label="Close history" onclick={onClose}>×</button>
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  {#if status?.state === "not-a-repo"}
    <div class="intro">
      <p>History keeps snapshots of your project so you can see what changed and bring back earlier versions. Snapshots are taken every 10 minutes while you work, and whenever you press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd>.</p>
      <p>It uses git: turning it on creates a git repository in the project folder.</p>
      <button class="primary" onclick={enable}>Turn on history</button>
    </div>
  {:else if status?.state === "no-git"}
    <p class="intro">{status.message}</p>
  {:else if diffFor}
    <div class="diffbar">
      <button onclick={() => (diffFor = null)}>← Back</button>
      <span>{diffFor.path}</span>
      {#if confirmRestore}
        <span class="confirm">Replace the current file? The current version is kept as a snapshot.</span>
        <button onclick={() => (confirmRestore = false)}>Cancel</button>
        <button class="primary" onclick={restore}>Restore</button>
      {:else}
        <button onclick={() => (confirmRestore = true)}>Restore this version</button>
      {/if}
    </div>
    <p class="legend">Red lines were in the snapshot, green lines are in the current version.</p>
    <div class="diff" bind:this={diffHost}></div>
  {:else if snapshots.length === 0}
    <p class="intro">No snapshots yet. The first one is taken after your next save.</p>
  {:else}
    <ul>
      {#each snapshots as s (s.id)}
        <li>
          <button class="snap" class:open={openId === s.id} onclick={() => (openId = openId === s.id ? null : s.id)}>
            <span class="label">{s.label}</span>
            <span class="time">{when(s.time)} · {s.changes.length} file{s.changes.length === 1 ? "" : "s"}</span>
          </button>
          {#if openId === s.id}
            <ul class="files">
              {#each s.changes as c (c.path)}
                <li>
                  <button onclick={() => showDiff(s.id, c.path)}>
                    <i class={c.status}>{c.status === "added" ? "A" : c.status === "deleted" ? "D" : "M"}</i>{c.path}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</aside>

<style>
  .history { display: flex; flex-direction: column; height: 100%; background: var(--sheet); min-width: 0; }
  .bar { display: flex; align-items: center; padding: 6px 12px; background: var(--chrome); border-bottom: 1px solid var(--rule); }
  .close { margin-left: auto; border: none; background: none; font-size: 16px; padding: 0 6px; }
  .intro { padding: 12px 14px; color: var(--muted); margin: 0; display: flex; flex-direction: column; gap: 10px; align-items: flex-start; max-width: 60ch; }
  .intro p { margin: 0; }
  .error { color: var(--mark); padding: 8px 14px; margin: 0; }
  ul { list-style: none; margin: 0; padding: 0; overflow: auto; }
  .snap { width: 100%; text-align: left; border: none; border-bottom: 1px solid var(--rule); border-radius: 0; background: none; display: flex; flex-direction: column; padding: 8px 14px; font-weight: 400; }
  .snap.open { background: var(--blue-soft); }
  .label { font-weight: 500; }
  .time { color: var(--muted); font: 11px var(--f-mono); }
  .files button { width: 100%; text-align: left; border: none; border-radius: 0; background: none; font: 12px var(--f-mono); padding: 4px 14px 4px 26px; display: flex; gap: 8px; }
  .files button:hover { background: color-mix(in srgb, var(--blue) 8%, transparent); }
  .files i { font-style: normal; width: 12px; }
  i.added { color: var(--ok); }
  i.deleted { color: var(--mark); }
  i.modified { color: var(--warn); }
  .diffbar { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; padding: 8px 12px; border-bottom: 1px solid var(--rule); }
  .diffbar > span { font: 12px var(--f-mono); color: var(--muted); }
  .confirm { color: var(--ink) !important; font-family: var(--f-body) !important; }
  .legend { margin: 0; padding: 4px 12px; color: var(--muted); font-size: 11px; }
  .diff { flex: 1; min-height: 0; overflow: hidden; }
  .diff :global(.cm-editor) { height: 100%; }
  kbd { font-size: 11px; border: 1px solid var(--rule); border-radius: 3px; padding: 0 4px; }
</style>
