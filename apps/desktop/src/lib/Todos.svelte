<script lang="ts">
  // The project's TODO, FIXME and XXX comments and \todo notes, grouped by file.
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import { groupTodos, todoApi, type Todo } from "./todos";

  interface Props {
    onOpen: (file: string, line: number) => void;
    /** Change it to reload the list, e.g. after a save. */
    refreshKey?: number;
  }
  let { onOpen, refreshKey = 0 }: Props = $props();

  let todos = $state<Todo[]>([]);
  let error = $state<string | null>(null);
  let busy = $state(false);
  const groups = $derived(groupTodos(todos));

  async function load() {
    busy = true;
    try {
      todos = await todoApi.list();
      error = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    void refreshKey;
    load();
  });
</script>

<div class="todos">
  <header>
    <h2>TODOs <span class="count">{todos.length}</span></h2>
    <button class="icon" onclick={load} disabled={busy} title="Refresh" aria-label="Refresh TODOs">
      <RefreshCw size={15} />
    </button>
  </header>

  <div class="list">
    {#if error}
      <p class="error">{error}</p>
    {:else if !busy && todos.length === 0}
      <p class="muted">No TODOs in the project.</p>
    {/if}

    {#each groups as group (group.file)}
      <section>
        <h3>{group.file} <span class="count">{group.todos.length}</span></h3>
        <ul>
          {#each group.todos as todo}
            <li>
              <button onclick={() => onOpen(todo.file, todo.line)} title="Go to line {todo.line}">
                <span class="kind">{todo.kind}</span>
                <span class="text">{todo.text || "(no text)"}</span>
                <span class="line">:{todo.line}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  </div>
</div>

<style>
  .todos { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--surface-2); }
  header {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    height: 40px; padding: 0 12px 0 14px; border-bottom: 1px solid var(--border); background: var(--surface); flex: none;
  }
  h2 { margin: 0; font: 600 13px var(--f-body); color: var(--ink); }
  h3 { margin: 0 0 4px; font: 600 12px var(--f-body); color: var(--muted); overflow-wrap: anywhere; }
  .count { font-weight: 500; color: var(--faint); }
  .icon {
    display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px;
    border: none; background: none; border-radius: 6px; color: var(--muted); cursor: pointer;
  }
  .icon:hover:not(:disabled) { background: var(--surface-3, var(--border)); color: var(--ink); }
  .icon:disabled { opacity: 0.5; cursor: default; }
  .list { overflow: auto; flex: 1; min-height: 0; padding: 10px 12px 16px; display: flex; flex-direction: column; gap: 12px; }
  section + section { border-top: 1px solid var(--border); padding-top: 10px; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  li button {
    display: flex; align-items: baseline; gap: 8px; width: 100%; padding: 5px 6px; border: none; background: none;
    border-radius: 5px; text-align: left; font: 13px/1.4 var(--f-body); color: var(--ink); cursor: pointer;
  }
  li button:hover { background: var(--surface-3, var(--border)); }
  .kind {
    flex: none; font: 600 10px var(--f-mono); letter-spacing: 0.03em; color: var(--accent);
    padding: 1px 5px; border: 1px solid currentColor; border-radius: 4px;
  }
  .text { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .line { flex: none; font: 12px var(--f-mono); color: var(--faint); }
  .muted { color: var(--muted); font-size: 13px; margin: 8px 2px; }
  .error { color: var(--danger, #b42318); font-size: 13px; margin: 8px 2px; }
</style>
