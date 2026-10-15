<script lang="ts">
  // Saves a project as a template for new projects; the caller trashes the project when asked.
  import Modal from "./ui/Modal.svelte";
  import { api, errorMessage } from "./api";

  interface Props {
    /** The project folder. */
    path: string;
    projectName: string;
    onSaved: (name: string, trash: boolean) => void;
    onClose: () => void;
  }
  let { path, projectName, onSaved, onClose }: Props = $props();
  // svelte-ignore state_referenced_locally
  let name = $state(projectName);
  let trash = $state(false);
  let description = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      await api.saveAsTemplate(path, name, description);
      onSaved(name.trim(), trash);
    } catch (err) {
      error = errorMessage(err);
      busy = false;
    }
  }
</script>

<Modal title="Save as template" subtitle="New projects can start from a copy of this one: all its files, except build output and history." width={480} {onClose}>
  <form id="save-template" onsubmit={save}>
    <div class="field">
      <label for="st-name">Name</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="st-name" class="input" bind:value={name} required autofocus />
    </div>
    <div class="field">
      <label for="st-desc">Description (optional)</label>
      <input id="st-desc" class="input" bind:value={description} placeholder="Exercise sheets with my usual header" />
    </div>
    <label class="check"><input type="checkbox" bind:checked={trash} /> Then move the project to the trash</label>
    <p class="hint">Remove any text you don't want in every new project before saving. Templates you save appear when you create a new project.</p>
    {#if error}<p class="error">{error}</p>{/if}
  </form>
  {#snippet footer()}
    <button class="btn" onclick={onClose}>Cancel</button>
    <button class="btn primary" type="submit" form="save-template" disabled={busy || !name.trim()}>Save template</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 12px; }
  .check { display: flex; align-items: center; gap: 8px; }
  .hint { color: var(--muted); font-size: 12.5px; margin: 0; }
  .error { color: var(--error); margin: 0; }
</style>
