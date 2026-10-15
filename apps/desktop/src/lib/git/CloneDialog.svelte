<script lang="ts">
  // Clone a repository from the start screen: a URL (https, ssh, or an
  // Overleaf project) and a folder to put it in.
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { documentDir } from "@tauri-apps/api/path";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Modal from "../ui/Modal.svelte";
  import { errorMessage } from "../api";
  import { gitApi, isOverleaf, repoName } from "./api";

  interface Props {
    defaultParent?: string;
    /** The folder the repository was cloned into. */
    onCloned: (path: string) => void;
    onClose: () => void;
  }
  let { defaultParent = "", onCloned, onClose }: Props = $props();

  let url = $state("");
  /** Typed by the user; while empty, the name follows the URL. */
  let typedName = $state("");
  let parent = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  const name = $derived(typedName || repoName(url));

  onMount(async () => {
    parent = defaultParent || (await documentDir().catch(() => ""));
  });

  async function chooseParent() {
    const dir = await open({ directory: true, title: "Choose where to put the project", defaultPath: parent || undefined });
    if (typeof dir === "string") parent = dir;
  }

  async function run(e?: SubmitEvent) {
    e?.preventDefault();
    busy = true;
    error = null;
    try {
      onCloned(await gitApi.clone(url, parent, name));
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Clone a repository" subtitle="Copies a git repository to this computer and opens it" width={540} {onClose}>
  <form id="clone-form" onsubmit={run}>
    <div class="field">
      <label for="clone-url">Repository URL</label>
      <input id="clone-url" class="input mono" bind:value={url} placeholder="https://github.com/you/thesis.git" spellcheck="false" autocomplete="off" required disabled={busy} />
      <small>https or ssh, such as <code>git@github.com:you/thesis.git</code>. Lemmata uses the system git and its saved credentials.</small>
    </div>
    {#if isOverleaf(url)}
      <p class="note">
        Overleaf's git access needs a token: create one under Overleaf account settings → Git integration. Enter the URL as
        <code>https://git:TOKEN@git.overleaf.com/ID</code>, or sign in once with <code>git clone</code> in a terminal so your credential manager remembers it.
        A token in the URL is kept in the project's git settings.
      </p>
    {/if}
    <div class="field">
      <label for="clone-parent">Location</label>
      <div class="row">
        <input id="clone-parent" class="input mono" bind:value={parent} required disabled={busy} />
        <button type="button" class="btn" onclick={chooseParent} disabled={busy}><FolderOpen size={15} /> Choose…</button>
      </div>
    </div>
    <div class="field">
      <label for="clone-name">Folder name</label>
      <input id="clone-name" class="input" value={name} oninput={(e) => (typedName = e.currentTarget.value)} required disabled={busy} />
    </div>
    {#if busy}<p class="muted">Cloning… this can take a minute for a large repository.</p>{/if}
    {#if error}<p class="error">{error}</p>{/if}
  </form>
  {#snippet footer()}
    <button class="btn" onclick={onClose}>Cancel</button>
    <button class="btn primary" type="submit" form="clone-form" disabled={busy || !url.trim() || !parent || !name.trim()}>{busy ? "Cloning…" : "Clone"}</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 14px; }
  p { margin: 0; }
  small, .muted { color: var(--muted); }
  small { display: block; margin-top: 4px; font-size: 12px; }
  .note { font-size: 12px; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r); padding: 8px 10px; }
  .row { display: flex; gap: 8px; }
  .row .input { flex: 1; min-width: 0; }
  .error { color: var(--error); white-space: pre-wrap; user-select: text; }
</style>
