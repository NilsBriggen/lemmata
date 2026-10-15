<script lang="ts">
  // Review comments on the project: open and resolved threads, replies, and
  // the form for a new comment on the selected text.
  import X from "@lucide/svelte/icons/x";
  import type { Thread } from "./model";

  interface Props {
    threads: Thread[];
    active: string | null;
    /** A new comment being written: the text it's about. */
    draft: { file: string; line: number; quote: string } | null;
    onAdd: (body: string) => void;
    onCancelDraft: () => void;
    onSelect: (t: Thread) => void;
    onReply: (id: string, body: string) => void;
    onResolve: (id: string, resolved: boolean) => void;
    onDelete: (id: string) => void;
    onClose: () => void;
  }
  let p: Props = $props();

  let showResolved = $state(false);
  let text = $state("");
  let replies = $state<Record<string, string>>({});
  let confirmDelete = $state<string | null>(null);

  const open = $derived(p.threads.filter((t) => !t.resolved));
  const shown = $derived([...(showResolved ? p.threads : open)].sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line));
  const when = (t: number) => new Date(t).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  const excerpt = (q: string) => (q.length > 160 ? `${q.slice(0, 160)}…` : q);

  function add(e: SubmitEvent) {
    e.preventDefault();
    if (!text.trim()) return;
    p.onAdd(text.trim());
    text = "";
  }

  function reply(e: SubmitEvent, id: string) {
    e.preventDefault();
    const body = replies[id]?.trim();
    if (!body) return;
    p.onReply(id, body);
    replies[id] = "";
  }
</script>

<aside class="review" aria-label="Review comments">
  <div class="bar">
    <b>Comments</b>
    <span class="count">{open.length} open</span>
    <label class="toggle"><input type="checkbox" bind:checked={showResolved} /> Show resolved</label>
    <button class="btn ghost icon sm close" aria-label="Close comments" onclick={p.onClose}><X size={15} /></button>
  </div>

  {#if p.draft}
    <form class="draft" onsubmit={add}>
      <span class="where">{p.draft.file}:{p.draft.line}</span>
      <blockquote>{excerpt(p.draft.quote)}</blockquote>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea class="input" rows="3" placeholder="Write a comment" bind:value={text} autofocus
        onkeydown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) (e.currentTarget.form as HTMLFormElement).requestSubmit();
          if (e.key === "Escape") p.onCancelDraft();
        }}></textarea>
      <div class="actions">
        <button type="button" class="btn sm" onclick={p.onCancelDraft}>Cancel</button>
        <button type="submit" class="btn sm primary" disabled={!text.trim()}>Comment</button>
      </div>
    </form>
  {/if}

  {#if !shown.length && !p.draft}
    <p class="empty">{p.threads.length ? "No open comments." : "No comments yet. Select some text and choose Add comment (in the editor toolbar or the command palette)."}</p>
  {/if}

  <ul>
    {#each shown as t (t.id)}
      <li class:active={t.id === p.active} class:resolved={t.resolved}>
        <button class="head" onclick={() => p.onSelect(t)} title="Show in the editor">
          <span class="where">{t.file}:{t.line}</span>
          <blockquote>{excerpt(t.quote)}</blockquote>
        </button>
        <p class="body">{t.body}</p>
        <span class="time">{when(t.created)}</span>
        {#each t.replies as r, i (i)}
          <div class="reply"><p class="body">{r.body}</p><span class="time">{when(r.created)}</span></div>
        {/each}
        {#if t.id === p.active && !t.resolved}
          <form class="reply-form" onsubmit={(e) => reply(e, t.id)}>
            <input class="input" placeholder="Reply" bind:value={replies[t.id]} />
          </form>
        {/if}
        <div class="actions">
          <button class="btn sm" onclick={() => p.onResolve(t.id, !t.resolved)}>{t.resolved ? "Reopen" : "Resolve"}</button>
          {#if confirmDelete === t.id}
            <button class="btn sm danger-btn" onclick={() => ((confirmDelete = null), p.onDelete(t.id))}>Delete for good</button>
          {:else}
            <button class="btn sm ghost" onclick={() => (confirmDelete = t.id)}>Delete</button>
          {/if}
        </div>
      </li>
    {/each}
  </ul>
</aside>

<style>
  .review { display: flex; flex-direction: column; height: 100%; background: var(--surface-2); min-width: 0; }
  .bar { display: flex; align-items: center; gap: 10px; height: 40px; padding: 0 8px 0 14px; background: var(--surface); border-bottom: 1px solid var(--border); flex: none; }
  .bar b { font-weight: 600; }
  .count { color: var(--muted); font-size: 12px; }
  .toggle { display: flex; align-items: center; gap: 5px; font-size: 12px; color: var(--muted); }
  .close { margin-left: auto; }
  .empty { padding: 12px 14px; color: var(--muted); margin: 0; }
  ul { list-style: none; margin: 0; padding: 0; overflow: auto; }
  li, .draft { display: flex; flex-direction: column; gap: 4px; padding: 10px 14px; border-bottom: 1px solid var(--border); background: var(--surface); }
  li.active { background: var(--accent-soft); }
  li.resolved { opacity: 0.7; }
  .head { text-align: left; border: none; background: none; padding: 0; display: flex; flex-direction: column; gap: 2px; cursor: pointer; }
  .where { font: 11.5px var(--f-mono); color: var(--muted); }
  blockquote { margin: 0; padding: 2px 8px; border-left: 3px solid rgba(220, 160, 20, 0.7); color: var(--ink); font: 12px var(--f-mono); white-space: pre-wrap; overflow-wrap: anywhere; }
  .body { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .time { color: var(--muted); font-size: 11.5px; }
  .reply { margin-left: 12px; padding-left: 8px; border-left: 2px solid var(--border); }
  .actions { display: flex; gap: 6px; justify-content: flex-end; }
  textarea { resize: vertical; font: inherit; }
  .danger-btn { background: var(--error) !important; border-color: var(--error) !important; color: #fff !important; }
</style>
