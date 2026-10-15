<script lang="ts">
  // Snapshots of the project, with a diff of any changed file against the
  // current version and a one-file restore.
  import { onDestroy } from "svelte";
  import { EditorState } from "@codemirror/state";
  import { EditorView, lineNumbers } from "@codemirror/view";
  import { unifiedMergeView } from "@codemirror/merge";
  import X from "@lucide/svelte/icons/x";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Pencil from "@lucide/svelte/icons/pencil";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Check from "@lucide/svelte/icons/check";
  import FileText from "@lucide/svelte/icons/file-text";
  import { api, errorMessage, type HistoryStatus, type Snapshot } from "./api";
  import { computeHunks, isTextPath, rejectHunks, type Hunk } from "./history/hunks";
  import { historyLatexdiff } from "./history/latexdiff";

  interface Props {
    /** Bumped by the app after it takes a snapshot, to reload the list. */
    version: number;
    /** Current contents of a file, including unsaved edits. */
    currentText: (path: string) => Promise<string | null>;
    /** Saves unsaved edits and stops pending autosaves, so a restore can't be overwritten by them. */
    beforeRestore: () => Promise<void>;
    onRestored: (path: string) => void;
    /** The whole project was put back; reload everything from disk. */
    onRestoredAll: () => void;
    onClose: () => void;
    /** Saves unsaved edits (and stops pending autosaves) before the review changes files or compares them. Defaults to `beforeRestore`. */
    beforeChange?: () => Promise<void>;
    /** Writes a file through the app's normal save path, so its open buffer, undo and conflict checks apply. Without it, hunks can't be rejected. */
    applyText?: (path: string, text: string) => Promise<void>;
    /** Opens the PDF of the changes (made by latexdiff). */
    onOpenPdf?: (path: string) => void;
  }
  let { version, currentText, beforeRestore, onRestored, onRestoredAll, onClose, beforeChange = beforeRestore, applyText, onOpenPdf }: Props = $props();

  let status = $state<HistoryStatus | null>(null);
  let snapshots = $state<Snapshot[]>([]);
  let openId = $state<string | null>(null);
  let diffFor = $state<{ id: string; path: string; other: string | null } | null>(null);
  /** Compare the open snapshot with another one instead of with the current files. */
  let compareId = $state<string | null>(null);
  let compared = $state<Snapshot["changes"]>([]);
  let editing = $state<{ id: string; text: string } | null>(null);
  let confirmAll = $state<string | null>(null);
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

  $effect(() => {
    const [a, b] = [openId, compareId];
    compared = [];
    if (a && b) api.historyCompare(a, b).then((c) => a === openId && b === compareId && (compared = c)).catch((e) => (error = errorMessage(e)));
  });

  async function showDiff(id: string, path: string) {
    const other = compareId;
    diffFor = { id, path, other };
    confirmRestore = false;
    const [then, now] = await Promise.all([api.historyFile(id, path), other ? api.historyFile(other, path) : currentText(path)]);
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
      await beforeRestore();
      await api.historyRestore(diffFor.id, diffFor.path);
      onRestored(diffFor.path);
      confirmRestore = false;
      diffFor = null;
      await load();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function saveLabel() {
    if (!editing) return;
    const { id, text } = editing;
    editing = null;
    if (!text.trim()) return;
    try {
      await api.historyRelabel(id, text);
      await load();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function restoreAll(id: string) {
    try {
      await beforeRestore();
      await api.historyRestoreAll(id);
      confirmAll = null;
      onRestoredAll();
      await load();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  // ---- Review changes: every changed file as hunks to accept or reject. ----
  interface ReviewFile { path: string; old: string; hunks: Hunk[] }
  const REVIEWED = "Reviewed";
  /** "" = since the last snapshot marked as reviewed. */
  let since = $state("");
  let review = $state<{ base: Snapshot; files: ReviewFile[] } | null>(null);
  /** Hunks kept as they are, as `path\0key`; they stay hidden while the review is open. */
  let accepted = $state(new Set<string>());
  let busy = $state<string | null>(null);
  let pdf = $state<{ id: string; path: string } | null>(null);
  const lastReviewed = $derived(snapshots.find((s) => s.label.startsWith(REVIEWED)));
  const baseSnap = $derived(since ? snapshots.find((s) => s.id === since) : lastReviewed);
  const pending = (f: ReviewFile) => f.hunks.filter((h) => !accepted.has(`${f.path}\0${h.key}`));
  const shown = $derived(review?.files.filter((f) => pending(f).length) ?? []);

  /** At most `n` lines, for showing a hunk. */
  function preview(text: string, n = 14) {
    const lines = text.replace(/\n$/, "").split("\n");
    return lines.length > n ? [...lines.slice(0, n), `… ${lines.length - n} more lines`].join("\n") : lines.join("\n");
  }

  async function startReview() {
    const base = baseSnap;
    if (!base) return;
    error = null;
    busy = "review";
    try {
      await beforeChange();
      // The work since the last snapshot counts too.
      if (await api.historySnapshot("Before review")) await load();
      const newer = snapshots.slice(0, Math.max(0, snapshots.findIndex((s) => s.id === base.id)));
      const paths = [...new Set(newer.flatMap((s) => s.changes.map((c) => c.path)))].filter(isTextPath).sort();
      const files: ReviewFile[] = [];
      for (const path of paths) {
        const [old, now] = await Promise.all([api.historyFile(base.id, path), currentText(path)]);
        const hunks = computeHunks(old ?? "", now ?? "");
        if (hunks.length) files.push({ path, old: old ?? "", hunks });
      }
      accepted = new Set();
      review = { base, files };
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = null;
    }
  }

  const accept = (f: ReviewFile, hs: Hunk[]) => (accepted = new Set([...accepted, ...hs.map((h) => `${f.path}\0${h.key}`)]));

  async function reject(f: ReviewFile, hs: Hunk[]) {
    if (!applyText || !review) return;
    busy = f.path;
    error = null;
    try {
      await beforeChange();
      // Read again: the file may have changed since the review opened.
      const now = (await currentText(f.path)) ?? "";
      const keys = new Set(hs.map((h) => h.key));
      await applyText(f.path, rejectHunks(now, computeHunks(f.old, now).filter((h) => keys.has(h.key))));
      const after = (await currentText(f.path)) ?? "";
      f.hunks = computeHunks(f.old, after);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = null;
    }
  }

  async function markReviewed() {
    error = null;
    busy = "mark";
    try {
      await beforeChange();
      if (!(await api.historySnapshot(REVIEWED))) {
        // Nothing changed since the newest snapshot: it is the reviewed state.
        const latest = snapshots[0];
        if (latest && !latest.label.startsWith(REVIEWED)) {
          await api.historyRelabel(latest.id, /^(Snapshot|Automatic snapshot)$/.test(latest.label) ? REVIEWED : `${REVIEWED}: ${latest.label}`);
        }
      }
      since = "";
      review = null;
      await load();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = null;
    }
  }

  async function makePdf() {
    const base = baseSnap;
    if (!base) return;
    error = null;
    busy = "pdf";
    try {
      await beforeChange();
      pdf = { id: base.id, path: await historyLatexdiff(base.id) };
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = null;
    }
  }

  const when = (t: number) =>
    new Date(t * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

  onDestroy(() => diffView?.destroy());
</script>

{#snippet reviewBar()}
  <div class="review">
    <label class="compare">
      Review changes since
      <select class="input" bind:value={since}>
        <option value="">{lastReviewed ? `my last review · ${when(lastReviewed.time)}` : "Choose a snapshot…"}</option>
        {#each snapshots as o (o.id)}
          <option value={o.id}>{o.label} · {when(o.time)}</option>
        {/each}
      </select>
    </label>
    <button class="btn sm" disabled={!baseSnap || !!busy} onclick={startReview}>{busy === "review" ? "Looking…" : "Review changes"}</button>
    <button class="btn sm ghost" disabled={!baseSnap || !!busy} onclick={makePdf} title="Compiles the old and the new version with latexdiff into one PDF with the changes marked.">
      <FileText size={13} /> {busy === "pdf" ? "Making the PDF…" : "PDF of the changes"}
    </button>
    {#if pdf && pdf.id === baseSnap?.id && onOpenPdf}
      <button class="btn sm primary" onclick={() => onOpenPdf(pdf!.path)}>Open PDF of the changes</button>
    {/if}
    <button class="btn sm ghost" disabled={!!busy} onclick={markReviewed} title="Takes a snapshot called Reviewed, where the next review starts."><Check size={13} /> Mark as reviewed</button>
  </div>
{/snippet}

<aside class="history">
  <div class="bar">
    <b>History</b>
    <button class="btn ghost icon sm close" aria-label="Close history" onclick={onClose}><X size={15} /></button>
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  {#if status?.state === "not-a-repo"}
    <div class="intro">
      <p>History keeps snapshots of your project so you can see what changed and bring back earlier versions. Snapshots are taken every 10 minutes while you work, and whenever you press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd>.</p>
      <p>It uses git and keeps the snapshots in the project's <code>.lemmata</code> folder; the project folder itself doesn't change.</p>
      <button class="btn primary" onclick={enable}>Turn on history</button>
    </div>
  {:else if status?.state === "no-git"}
    <p class="intro">{status.message}</p>
  {:else if diffFor}
    <div class="diffbar">
      <button class="btn sm ghost" onclick={() => (diffFor = null)}><ArrowLeft size={14} /> Back</button>
      <span>{diffFor.path}</span>
      {#if confirmRestore}
        <span class="confirm">Replace the current file? The current version is kept as a snapshot.</span>
        <button class="btn sm" onclick={() => (confirmRestore = false)}>Cancel</button>
        <button class="btn sm primary" onclick={restore}>Restore</button>
      {:else}
        <button class="btn sm" onclick={() => (confirmRestore = true)}>Restore this version</button>
      {/if}
    </div>
    <p class="legend">Red lines were in the snapshot, green lines are in {diffFor.other ? "the other snapshot" : "the current version"}.</p>
    <div class="diff" bind:this={diffHost}></div>
  {:else if review}
    <div class="diffbar">
      <button class="btn sm ghost" onclick={() => (review = null)}><ArrowLeft size={14} /> Back</button>
      <span>Changes since {review.base.label} · {when(review.base.time)}</span>
      <button class="btn sm ghost" disabled={!shown.length} onclick={() => shown.forEach((f) => accept(f, f.hunks))}>Accept all</button>
      <button class="btn sm ghost" disabled={!shown.length || !applyText || !!busy} onclick={async () => { for (const f of shown) await reject(f, pending(f)); }}>Reject all</button>
      <button class="btn sm" disabled={!!busy} onclick={markReviewed}><Check size={13} /> Mark as reviewed</button>
    </div>
    <p class="legend">Accept keeps the current text. Reject puts the snapshot's text back for that change only; undo it with Ctrl+Z in the file.</p>
    <div class="hunks">
      {#each shown as f (f.path)}
        <section>
          <header>
            <b>{f.path}</b>
            <span class="count">{pending(f).length} change{pending(f).length === 1 ? "" : "s"}</span>
            <button class="btn sm ghost" onclick={() => accept(f, f.hunks)}>Accept all</button>
            <button class="btn sm ghost" disabled={!applyText || busy === f.path} onclick={() => reject(f, pending(f))}>Reject all</button>
          </header>
          {#each pending(f) as h (h.key)}
            <div class="hunk">
              <div class="hunk-head">
                <span>line {h.line}</span>
                <button class="btn sm ghost" onclick={() => accept(f, [h])}>Accept</button>
                <button class="btn sm ghost" disabled={!applyText || busy === f.path} onclick={() => reject(f, [h])}>Reject</button>
              </div>
              {#if h.oldText}<pre class="del">{preview(h.oldText)}</pre>{/if}
              {#if h.newText}<pre class="add">{preview(h.newText)}</pre>{/if}
            </div>
          {/each}
        </section>
      {:else}
        <p class="intro">{review.files.length ? "You have been through every change." : "Nothing changed since then."}</p>
      {/each}
    </div>
  {:else if snapshots.length === 0}
    <p class="intro">No snapshots yet. The first one is taken after your next save.</p>
  {:else}
    {@render reviewBar()}
    <ul>
      {#each snapshots as s (s.id)}
        <li>
          <button class="snap" class:open={openId === s.id} onclick={() => ((openId = openId === s.id ? null : s.id), (compareId = null), (confirmAll = null))}>
            {#if editing?.id === s.id}
              <!-- svelte-ignore a11y_autofocus -->
              <input class="rename input" bind:value={editing.text} autofocus onclick={(e) => e.stopPropagation()} onkeydown={(e) => (e.stopPropagation(), e.key === "Enter" ? saveLabel() : e.key === "Escape" && (editing = null))} onblur={saveLabel} aria-label="Snapshot name" />
            {:else}
              <span class="snap-label">{s.label}</span>
            {/if}
            <span class="time">{when(s.time)} · {s.changes.length} file{s.changes.length === 1 ? "" : "s"}</span>
          </button>
          {#if openId === s.id}
            <div class="tools">
              <button class="btn sm ghost" onclick={() => (editing = { id: s.id, text: s.label })}><Pencil size={13} /> Rename</button>
              {#if confirmAll === s.id}
                <span class="confirm">Replace all files? Your current work is saved as a snapshot first.</span>
                <button class="btn sm" onclick={() => (confirmAll = null)}>Cancel</button>
                <button class="btn sm primary" onclick={() => restoreAll(s.id)}>Restore</button>
              {:else}
                <button class="btn sm ghost" onclick={() => (confirmAll = s.id)}><RotateCcw size={13} /> Restore project</button>
              {/if}
              <label class="compare">
                Compare with
                <select class="input" bind:value={compareId}>
                  <option value={null}>this snapshot's own changes</option>
                  {#each snapshots.filter((o) => o.id !== s.id) as o (o.id)}
                    <option value={o.id}>{o.label} · {when(o.time)}</option>
                  {/each}
                </select>
              </label>
            </div>
            <ul class="files">
              {#each compareId ? compared : s.changes as c (c.path)}
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
  .history { display: flex; flex-direction: column; height: 100%; background: var(--surface-2); min-width: 0; }
  .bar { display: flex; align-items: center; height: 40px; padding: 0 8px 0 14px; background: var(--surface); border-bottom: 1px solid var(--border); flex: none; }
  .bar b { font-weight: 600; }
  .close { margin-left: auto; }
  .intro { padding: 12px 14px; color: var(--muted); margin: 0; display: flex; flex-direction: column; gap: 10px; align-items: flex-start; max-width: 60ch; }
  .intro p { margin: 0; }
  .error { color: var(--error); padding: 8px 14px; margin: 0; white-space: pre-line; }
  ul { list-style: none; margin: 0; padding: 0; overflow: auto; }
  .snap { width: 100%; text-align: left; border: none; border-bottom: 1px solid var(--border); background: var(--surface); display: flex; flex-direction: column; gap: 2px; padding: 10px 14px; }
  .snap:hover { background: var(--surface-3); }
  .snap.open { background: var(--accent-soft); }
  .snap-label { font-weight: 500; color: var(--ink); }
  .time { color: var(--muted); font-size: 12px; }
  .files button { width: 100%; text-align: left; border: none; border-radius: 0; background: none; font: 12px var(--f-mono); padding: 4px 14px 4px 26px; display: flex; gap: 8px; }
  .files button:hover { background: color-mix(in srgb, var(--accent) 8%, transparent); }
  .files i { font-style: normal; width: 12px; }
  i.added { color: var(--ok); }
  i.deleted { color: var(--error); }
  i.modified { color: var(--warn); }
  .tools { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 6px 12px; background: var(--surface); border-bottom: 1px solid var(--border); }
  .tools .confirm { font-size: 12px; color: var(--ink); }
  .compare { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--muted); }
  .compare .input { max-width: 220px; height: 26px; font-size: 12px; }
  .rename { height: 24px; font-size: 13px; }
  .diffbar { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; padding: 8px 12px; border-bottom: 1px solid var(--border); }
  .diffbar > span { font: 12px var(--f-mono); color: var(--muted); }
  .confirm { color: var(--ink) !important; font-family: var(--f-body) !important; }
  .legend { margin: 0; padding: 4px 12px; color: var(--muted); font-size: 11px; }
  .diff { flex: 1; min-height: 0; overflow: hidden; }
  .diff :global(.cm-editor) { height: 100%; }
  .review { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 8px 12px; background: var(--surface); border-bottom: 1px solid var(--border); flex: none; }
  .hunks { flex: 1; min-height: 0; overflow: auto; }
  .hunks section { border-bottom: 1px solid var(--border); }
  .hunks header { display: flex; align-items: center; gap: 6px; padding: 6px 12px; background: var(--surface); position: sticky; top: 0; }
  .hunks header b { font: 600 12px var(--f-mono); margin-right: auto; overflow-wrap: anywhere; }
  .count { font-size: 11px; color: var(--muted); }
  .hunk { padding: 4px 12px 8px; }
  .hunk-head { display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--muted); }
  .hunk-head span { margin-right: auto; }
  .hunk pre { margin: 0; padding: 3px 8px; font: 12px/1.45 var(--f-mono); white-space: pre-wrap; overflow-wrap: anywhere; border-radius: 3px; }
  .hunk pre.del { background: color-mix(in srgb, var(--error) 14%, transparent); }
  .hunk pre.add { background: color-mix(in srgb, var(--ok) 14%, transparent); margin-top: 2px; }
  kbd { font-size: 11px; border: 1px solid var(--border); border-radius: 3px; padding: 0 4px; }
</style>
