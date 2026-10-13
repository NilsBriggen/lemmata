<script lang="ts">
  // Saves the open project as a template for new projects.
  import Modal from "./ui/Modal.svelte";
  import { api, errorMessage } from "./api";

  let { projectName, onSaved, onClose }: { projectName: string; onSaved: (name: string) => void; onClose: () => void } = $props();
  // svelte-ignore state_referenced_locally
  let name = $state(projectName);
  let description = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      await api.saveAsTemplate(name, description);
      onSaved(name.trim());
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
  .hint { color: var(--muted); font-size: 12.5px; margin: 0; }
  .error { color: var(--error); margin: 0; }
</style>
