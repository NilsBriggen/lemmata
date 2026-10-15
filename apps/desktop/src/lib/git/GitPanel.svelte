<script lang="ts">
  // The project's git repository: status, commit, pull/push, branches and
  // recent commits. Everything runs through the system git.
  import { onMount } from "svelte";
  import X from "@lucide/svelte/icons/x";
  import Plus from "@lucide/svelte/icons/plus";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import { errorMessage } from "../api";
  import { aheadBehind, commitBlocker, gitApi, type Commit, type FileChange, type Overview } from "./api";

  interface Props {
    /** Bumped by the app to reload (for example after a save). */
    version?: number;
    /** Files or the branch changed on disk (switch, pull, first commit): refresh the tree and reload open files. */
    onChanged?: () => void;
    /** Save unsaved edits before git looks at the files. */
    beforeGit?: () => Promise<void>;
    onClose: () => void;
  }
  let { version = 0, onChanged, beforeGit, onClose }: Props = $props();

  let overview = $state<Overview | null>(null);
  let branches = $state<string[]>([]);
  let commits = $state<Commit[]>([]);
  let message = $state("");
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let info = $state<string | null>(null);
  let newBranch = $state<string | null>(null);

  const repo = $derived(overview?.state === "repo" ? overview : null);
  const blocker = $derived(repo ? commitBlocker(repo, message) : null);
  const dirty = $derived(!!repo && repo.staged.length + repo.unstaged.length + repo.conflicted.length > 0);

  async function load() {
    try {
      overview = await gitApi.overview();
      if (overview.state === "repo") [branches, commits] = await Promise.all([gitApi.branches(), gitApi.log()]);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  onMount(load);
  $effect(() => {
    void version;
    if (!busy) load();
  });

  /** Runs one git action: shows what it is doing, reports the outcome, reloads. */
  async function act(label: string, f: () => Promise<string | void>, changed = false) {
    if (busy) return;
    busy = label;
    error = info = null;
    try {
      await beforeGit?.();
      const out = await f();
      if (out) info = out;
      if (changed) onChanged?.();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = null;
      await load();
    }
  }

  const toggle = (c: FileChange, staged: boolean) =>
    act("Updating…", () => (staged ? gitApi.unstage(c.from ? [c.path, c.from] : [c.path]) : gitApi.stage([c.path])));

  const commit = () =>
    act("Committing…", async () => {
      await gitApi.commit(message);
      message = "";
      return "Committed.";
    }, true);

  const initRepo = () => act("Setting up git…", async () => (await gitApi.init(), "This project is now a git repository."), true);

  function onSwitch(e: Event & { currentTarget: HTMLSelectElement }) {
    const name = e.currentTarget.value;
    act("Switching…", () => gitApi.switchBranch(name), true);
  }

  function createBranch(e: SubmitEvent) {
    e.preventDefault();
    const name = newBranch ?? "";
    act("Creating branch…", async () => {
      await gitApi.createBranch(name);
      newBranch = null;
    }, true);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && !blocker && !busy) {
      e.preventDefault();
      commit();
    }
  }
</script>

{#snippet files(title: string, list: FileChange[], staged: boolean)}
  {#if list.length}
    <h4>{title} <i>{list.length}</i>
      {#if !staged}<button class="btn sm ghost" disabled={!!busy} onclick={() => act("Staging…", gitApi.stageAll)}>Stage all</button>{/if}
    </h4>
    <ul class="files">
      {#each list as c (c.path)}
        <li>
          <label title={c.from ? `${c.from} → ${c.path}` : c.path}>
            <input type="checkbox" checked={staged} disabled={!!busy} onchange={() => toggle(c, staged)} />
            <b class="st s{c.status === '?' ? 'u' : c.status}">{c.status}</b>
            <span class="path">{c.path}</span>
          </label>
        </li>
      {/each}
    </ul>
  {/if}
{/snippet}

<aside class="git">
  <div class="bar">
    <b>Git</b>
    {#if busy}<span class="busy">{busy}</span>{/if}
    <span class="grow"></span>
    <button class="btn ghost icon sm" aria-label="Refresh" title="Refresh" disabled={!!busy} onclick={() => act("Refreshing…", async () => {})}><RefreshCw size={14} /></button>
    <button class="btn ghost icon sm" aria-label="Close Git" onclick={onClose}><X size={15} /></button>
  </div>

  {#if error}<p class="error">{error}</p>{/if}
  {#if info}<p class="info">{info}</p>{/if}

  {#if !overview}
    <p class="intro">Loading…</p>
  {:else if overview.state === "no-git"}
    <p class="intro">{overview.message}</p>
  {:else if overview.state === "not-a-repo"}
    <div class="intro">
      <p>This project isn't in a git repository. A repository keeps every commit you make, lets you go back to any of them, and lets you sync with GitHub, GitLab or Overleaf.</p>
      <button class="btn primary" disabled={!!busy} onclick={initRepo}>Turn this project into a git repository</button>
    </div>
  {:else if repo}
    <div class="scroll">
      <section class="branch">
        <GitBranch size={14} />
        <select class="input" aria-label="Branch" value={repo.branch ?? ""} disabled={!!busy} onchange={onSwitch}>
          {#if !repo.branch}<option value="" disabled>(detached HEAD)</option>{/if}
          {#each branches as b (b)}<option value={b}>{b}</option>{/each}
        </select>
        <button class="btn ghost icon sm" aria-label="New branch" title="New branch from here" disabled={!!busy} onclick={() => (newBranch = newBranch === null ? "" : null)}><Plus size={14} /></button>
        <span class="sync" title={repo.upstream ? `Tracking ${repo.upstream}` : "Not pushed yet"}>{repo.upstream ? aheadBehind(repo) || "up to date" : "no upstream"}</span>
      </section>
      {#if dirty && branches.length > 1}<p class="hint">Commit your changes before switching branches.</p>{/if}
      {#if newBranch !== null}
        <form class="newbranch" onsubmit={createBranch}>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="input" bind:value={newBranch} placeholder="New branch name" aria-label="New branch name" autofocus onkeydown={(e) => e.key === "Escape" && (e.stopPropagation(), (newBranch = null))} />
          <button class="btn sm primary" type="submit" disabled={!!busy || !newBranch.trim()}>Create</button>
        </form>
      {/if}

      <section class="syncrow">
        <button class="btn sm" disabled={!!busy} onclick={() => act("Fetching…", gitApi.fetch)}>Fetch</button>
        <button class="btn sm" disabled={!!busy} onclick={() => act("Pulling…", gitApi.pull, true)}><ArrowDown size={13} /> Pull{repo.behind ? ` ${repo.behind}` : ""}</button>
        <button class="btn sm" disabled={!!busy} onclick={() => act("Pushing…", gitApi.push)}><ArrowUp size={13} /> Push{repo.ahead ? ` ${repo.ahead}` : ""}</button>
      </section>

      <section class="changes">
        {#if !repo.staged.length && !repo.unstaged.length && !repo.untracked.length && !repo.conflicted.length}
          <p class="clean">No changes. Everything is committed.</p>
        {/if}
        {#if repo.conflicted.length}
          <h4>Conflicts <i>{repo.conflicted.length}</i></h4>
          <p class="hint">Resolve these in a terminal, then stage them.</p>
          <ul class="files">{#each repo.conflicted as c (c.path)}<li><div class="row"><b class="st sU">U</b><span class="path">{c.path}</span></div></li>{/each}</ul>
        {/if}
        {@render files("Staged", repo.staged, true)}
        {@render files("Changes", repo.unstaged, false)}
        {@render files("Untracked", repo.untracked, false)}
      </section>

      <section class="commitbox">
        <textarea class="input" rows="3" placeholder="Commit message (Ctrl+Enter to commit)" aria-label="Commit message" bind:value={message} onkeydown={onKey} disabled={!!busy}></textarea>
        <button class="btn primary" disabled={!!blocker || !!busy} title={blocker ?? ""} onclick={commit}>Commit{repo.staged.length ? ` ${repo.staged.length} file${repo.staged.length === 1 ? "" : "s"}` : ""}</button>
      </section>

      <details class="log" open>
        <summary>History <i>{commits.length}</i></summary>
        {#if !commits.length}<p class="hint">No commits yet.</p>{/if}
        <ul>
          {#each commits as c (c.hash)}
            <li>
              <span class="subject" title={c.subject}>{c.subject}</span>
              <span class="meta"><code>{c.hash}</code> · {c.author} · {c.date}</span>
            </li>
          {/each}
        </ul>
      </details>
    </div>
  {/if}
</aside>

<style>
  .git { display: flex; flex-direction: column; height: 100%; background: var(--surface-2); min-width: 0; }
  .bar { display: flex; align-items: center; gap: 4px; height: 40px; padding: 0 8px 0 14px; background: var(--surface); border-bottom: 1px solid var(--border); flex: none; }
  .bar b { font-weight: 600; }
  .busy { margin-left: 8px; color: var(--muted); font-size: 12px; }
  .grow { flex: 1; }
  .scroll { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; }
  .intro { padding: 12px 14px; color: var(--muted); margin: 0; display: flex; flex-direction: column; gap: 10px; align-items: flex-start; max-width: 60ch; }
  .intro p { margin: 0; }
  .error, .info { margin: 0; padding: 8px 14px; white-space: pre-wrap; user-select: text; overflow-wrap: anywhere; font-size: 12.5px; }
  .error { color: var(--error); }
  .info { color: var(--muted); }
  .hint { margin: 0; padding: 4px 14px; font-size: 12px; color: var(--muted); }
  section { padding: 8px 12px; border-bottom: 1px solid var(--border); }
  .branch { display: flex; align-items: center; gap: 6px; color: var(--muted); }
  .branch select { flex: 1; min-width: 0; height: 28px; }
  .sync { font-size: 12px; white-space: nowrap; }
  .newbranch { display: flex; gap: 6px; padding: 0 12px 8px; border-bottom: 1px solid var(--border); }
  .newbranch .input { flex: 1; min-width: 0; height: 28px; }
  .syncrow { display: flex; gap: 6px; }
  h4 { display: flex; align-items: center; gap: 6px; margin: 6px 0 2px; font-size: 11px; text-transform: uppercase; letter-spacing: .04em; color: var(--muted); font-weight: 600; }
  h4 i, summary i { font-style: normal; font-weight: 400; }
  h4 .btn { margin-left: auto; text-transform: none; letter-spacing: 0; }
  .clean { margin: 4px 0; color: var(--muted); font-size: 12.5px; }
  ul { list-style: none; margin: 0; padding: 0; }
  .files label, .files .row { display: flex; align-items: center; gap: 8px; padding: 3px 2px; cursor: pointer; font: 12px var(--f-mono); }
  .files label:hover { background: color-mix(in srgb, var(--accent) 8%, transparent); }
  .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
  .st { width: 12px; text-align: center; flex: none; }
  .sM { color: var(--warn); }
  .sA, .su { color: var(--ok); }
  .sD, .sU { color: var(--error); }
  .sR { color: var(--accent); }
  .commitbox { display: flex; flex-direction: column; gap: 6px; }
  .commitbox textarea { resize: vertical; min-height: 56px; font-family: inherit; }
  .log { padding: 8px 12px; }
  .log summary { cursor: pointer; font-size: 11px; text-transform: uppercase; letter-spacing: .04em; color: var(--muted); font-weight: 600; }
  .log li { padding: 5px 0; border-bottom: 1px solid var(--border); display: flex; flex-direction: column; gap: 1px; }
  .subject { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { font-size: 11px; color: var(--muted); }
</style>
