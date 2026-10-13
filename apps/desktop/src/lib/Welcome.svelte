<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";

  interface Props {
    recent: string[];
    error: string | null;
    onOpen: (path: string) => void;
  }
  let { recent, error, onOpen }: Props = $props();

  async function pick() {
    const dir = await open({ directory: true, title: "Open a LaTeX project folder" });
    if (typeof dir === "string") onOpen(dir);
  }

  const name = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
</script>

<div class="welcome">
  <div class="card">
    <h1>Lemmata</h1>
    <p class="lede">Open a folder that contains a LaTeX project. Lemmata builds it with your TeX installation and keeps everything in that folder.</p>
    <button class="primary" onclick={pick}>Open folder…</button>
    {#if error}<p class="error">{error}</p>{/if}
    {#if recent.length}
      <h2>Recent</h2>
      <ul>
        {#each recent as r (r)}
          <li><button onclick={() => onOpen(r)}><b>{name(r)}</b><span>{r}</span></button></li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .welcome { height: 100%; display: grid; place-items: center; padding: 24px; }
  .card { width: min(560px, 100%); display: flex; flex-direction: column; gap: 14px; align-items: flex-start; }
  h1 { font: 400 40px var(--f-display); margin: 0; }
  h2 { font: 500 11px var(--f-mono); letter-spacing: 0.12em; text-transform: uppercase; color: var(--muted); margin: 18px 0 0; }
  .lede { color: var(--muted); margin: 0; font-size: 14px; max-width: 52ch; }
  .error { color: var(--mark); margin: 0; }
  ul { list-style: none; margin: 0; padding: 0; width: 100%; display: flex; flex-direction: column; gap: 4px; }
  li button { width: 100%; text-align: left; display: flex; flex-direction: column; gap: 1px; padding: 8px 12px; font-weight: 400; }
  li span { font: 11px var(--f-mono); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
</style>
