<script lang="ts">
  // A shell command with Copy and, when possible, Open terminal.
  import { api, errorMessage, type InstallRequest } from "./api";

  interface Props {
    command: string;
    /** What to install if the user clicks Open terminal; the backend builds the command. */
    install?: InstallRequest;
  }
  let { command, install }: Props = $props();

  let copied = $state(false);
  let error = $state<string | null>(null);
  let codeEl: HTMLElement;

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      window.getSelection()?.selectAllChildren(codeEl);
    }
  }

  async function openTerminal() {
    error = null;
    try {
      await api.openInstallTerminal(install!);
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<div class="box">
  <code bind:this={codeEl}>{command}</code>
  <div class="actions">
    <button onclick={copy}>{copied ? "Copied" : "Copy"}</button>
    {#if install}<button onclick={openTerminal}>Open terminal</button>{/if}
  </div>
</div>
{#if error}<p class="error">{error}</p>{/if}

<style>
  .box { background: #0f1418; color: #d5dde3; border-radius: 6px; padding: 10px 12px; display: flex; gap: 12px; align-items: flex-start; }
  code { flex: 1; font-size: 12.5px; line-height: 1.6; white-space: pre-wrap; word-break: break-word; user-select: text; }
  .actions { display: flex; gap: 6px; flex: none; }
  .actions button { background: #1d252c; color: #d5dde3; border-color: #2c343d; }
  .error { color: var(--error); margin: 4px 0 0; font-size: 12px; }
</style>
