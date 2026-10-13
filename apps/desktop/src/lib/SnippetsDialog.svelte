<script lang="ts">
  // Edit the user's own snippets: a trigger, what it inserts, and a note.
  import Plus from "@lucide/svelte/icons/plus";
  import Trash2 from "@lucide/svelte/icons/trash";
  import Modal from "./ui/Modal.svelte";
  import { settings, updateSettings } from "./settings.svelte";
  import { validTrigger, type Snippet } from "./editor/snippets";

  let { onClose }: { onClose: () => void } = $props();
  let list = $state<Snippet[]>(settings.snippets.map((s) => ({ ...s })));

  const invalid = (s: Snippet, i: number) =>
    !validTrigger(s.trigger) || !s.body.trim() || list.findIndex((o) => o.trigger === s.trigger) !== i;
  const problems = $derived(list.filter(invalid).length);

  function add() {
    list = [...list, { trigger: "", body: "", description: "" }];
  }

  async function save() {
    await updateSettings({ snippets: list.filter((s, i) => !invalid(s, i)).map((s) => ({ ...s, trigger: s.trigger.trim() })) });
    onClose();
  }
</script>

<Modal title="Snippets" subtitle="Type a trigger in the editor and pick it from the completion list." width={640} {onClose}>
  <p class="help">
    Use <code>${"{}"}</code> for places the cursor stops at (Tab jumps to the next one), for example
    <code>\begin&#123;theorem&#125;${"{}"}\end&#123;theorem&#125;</code>. Triggers are two or more letters, digits or dashes.
  </p>
  {#if !list.length}<p class="empty">No snippets yet.</p>{/if}
  <ul>
    {#each list as s, i (i)}
      <li class:bad={invalid(s, i)}>
        <div class="top">
          <input class="input mono trig" bind:value={s.trigger} placeholder="trigger" aria-label="Trigger" spellcheck="false" />
          <input class="input" bind:value={s.description} placeholder="Description (optional)" aria-label="Description" />
          <button class="btn icon" aria-label="Delete snippet" onclick={() => (list = list.filter((_, j) => j !== i))}><Trash2 size={15} /></button>
        </div>
        <textarea class="input mono" rows="3" bind:value={s.body} placeholder={"\\begin{figure}[t]\n  ${}\n\\end{figure}"} aria-label="Text to insert" spellcheck="false"></textarea>
      </li>
    {/each}
  </ul>
  <button class="btn" onclick={add}><Plus size={15} /> New snippet</button>
  {#snippet footer()}
    {#if problems}<span class="warn">{problems} snippet{problems === 1 ? " is" : "s are"} incomplete or repeated and will be skipped.</span>{/if}
    <button class="btn" onclick={onClose}>Cancel</button>
    <button class="btn primary" onclick={save}>Save</button>
  {/snippet}
</Modal>

<style>
  .help { color: var(--muted); margin: 0 0 12px; font-size: 13px; line-height: 1.5; }
  .help code { font-family: var(--f-mono); font-size: 12px; background: var(--surface-2); padding: 1px 4px; border-radius: 3px; }
  .empty { color: var(--muted); }
  ul { list-style: none; margin: 0 0 12px; padding: 0; display: flex; flex-direction: column; gap: 12px; max-height: 50vh; overflow: auto; }
  li { display: flex; flex-direction: column; gap: 6px; padding: 10px; border: 1px solid var(--border); border-radius: var(--r); }
  li.bad { border-color: var(--warn); }
  .top { display: flex; gap: 6px; }
  .trig { width: 130px; flex: none; }
  .top .input:not(.trig) { flex: 1; }
  textarea { resize: vertical; }
  .warn { margin-right: auto; color: var(--warn); font-size: 12.5px; }
</style>
