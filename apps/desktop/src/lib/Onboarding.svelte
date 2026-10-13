<script lang="ts">
  // Shown when no usable TeX installation was found. Lemmata never runs
  // the install itself: it shows the command for this system and checks again.
  import { open } from "@tauri-apps/plugin-dialog";
  import { untrack } from "svelte";
  import type { TexStatus } from "./api";
  import CommandBox from "./CommandBox.svelte";

  interface Props {
    status: TexStatus;
    checking: boolean;
    onRecheck: (extraDir?: string) => void;
  }
  let { status, checking, onRecheck }: Props = $props();

  const order = ["arch", "debian", "fedora", "suse", "macos", "windows", "other"];
  // Starts on this computer's platform; the user can switch tabs.
  let platform = $state(untrack(() => status.os.family as string));
  let size = $state<"recommended" | "full">("recommended");

  const inst = $derived(status.instructions[platform] ?? status.instructions.other);
  const command = $derived(size === "recommended" ? inst.recommended : inst.full);
  const isCommand = $derived(!command.startsWith("Install "));
  const found = $derived(status.detection.tools.filter((t) => t.path).length);


  async function chooseFolder() {
    const dir = await open({ directory: true, title: "Choose the folder that contains pdflatex and latexmk" });
    if (typeof dir === "string") onRecheck(dir);
  }
</script>

<div class="onb">
  <aside class="steps">
    <span class="brand">Lemmata</span>
    <ol>
      <li class="done">Welcome</li>
      <li class="on">TeX Live</li>
      <li>First project</li>
    </ol>
  </aside>

  <main>
    <h1>{found === 0 ? "TeX Live isn't installed yet" : "TeX Live is incomplete"}</h1>
    <p class="lede">
      Lemmata uses the TeX installation on your computer to build PDFs. We looked in your PATH and the usual install
      folders{found === 0 ? " and found nothing" : ", but some required tools are missing"}. Install it with the command below,
      then click Check again.
    </p>

    <div class="checks">
      {#each status.detection.tools as t (t.name)}
        <div class="chk" class:yes={t.path} class:no={!t.path} class:optional={!t.required}>
          <b>{t.path ? "✓" : "✗"} {t.name}</b>
          <span>{t.path ? "found" : t.required ? "required" : "optional"}</span>
        </div>
      {/each}
    </div>

    <div class="tabs" role="tablist">
      {#each order.filter((k) => status.instructions[k]) as k (k)}
        <button role="tab" aria-selected={platform === k} class:on={platform === k} onclick={() => (platform = k)}>
          {status.instructions[k].label}{#if k === status.os.family}<i> · this computer</i>{/if}
        </button>
      {/each}
    </div>

    {#if inst.recommended !== inst.full}
      <div class="size">
        <button class:on={size === "recommended"} onclick={() => (size = "recommended")}>
          <b>Recommended {inst.recommended_size}</b><span>Covers almost every paper, thesis and Beamer deck</span>
        </button>
        <button class:on={size === "full"} onclick={() => (size = "full")}>
          <b>Full {inst.full_size}</b><span>Every package, so you're never asked to install more</span>
        </button>
      </div>
    {/if}

    {#if isCommand}
      <CommandBox {command} install={{ what: "tex", platform, full: size === "full" }} />
    {:else}
      <p class="note">{command}</p>
    {/if}
    {#if inst.note}<p class="note">{inst.note}</p>{/if}
    {#if inst.link}<p class="note">Installer and instructions: <code>{inst.link}</code></p>{/if}
    <p class="note">Open terminal shows the command and waits for you to press Enter. Lemmata never installs anything by itself.</p>

    <footer>
      <span>Installed somewhere else? <button class="link" onclick={chooseFolder}>Choose the TeX bin folder</button></span>
      <button class="primary" disabled={checking} onclick={() => onRecheck()}>{checking ? "Checking…" : "Check again"}</button>
    </footer>
  </main>
</div>

<style>
  .onb { display: grid; grid-template-columns: 220px 1fr; height: 100%; }
  .steps { background: var(--chrome); border-right: 1px solid var(--rule); padding: 24px 16px; }
  .brand { font: 20px var(--f-display); display: block; margin: 0 0 24px 10px; }
  ol { list-style: none; margin: 0; padding: 0; counter-reset: s; display: flex; flex-direction: column; gap: 4px; }
  li { counter-increment: s; padding: 7px 10px; border-radius: 5px; color: var(--muted); display: flex; gap: 10px; align-items: center; }
  li::before {
    content: counter(s); width: 20px; height: 20px; border-radius: 50%; border: 1px solid var(--rule);
    display: grid; place-items: center; font: 500 11px var(--f-mono);
  }
  li.done::before { content: "✓"; color: var(--ok); border-color: var(--ok); }
  li.on { background: var(--sheet); color: var(--ink); font-weight: 500; }
  li.on::before { border-color: var(--blue); color: var(--blue); }

  main { padding: 36px 44px; overflow: auto; display: flex; flex-direction: column; gap: 16px; max-width: 980px; }
  h1 { font: 400 26px var(--f-display); margin: 0; }
  .lede { color: var(--muted); max-width: 64ch; margin: 0; font-size: 14px; }
  .checks { display: grid; grid-template-columns: repeat(auto-fill, minmax(130px, 1fr)); gap: 8px; }
  .chk { border: 1px solid var(--rule); border-radius: 6px; padding: 8px 10px; display: flex; flex-direction: column; font: 12px var(--f-mono); background: var(--sheet); }
  .chk span { color: var(--muted); font-size: 11px; }
  .chk.yes b { color: var(--ok); }
  .chk.no:not(.optional) b { color: var(--mark); }
  .chk.no.optional b { color: var(--muted); }
  .tabs { display: flex; flex-wrap: wrap; border-bottom: 1px solid var(--rule); }
  .tabs button { border: none; border-radius: 0; background: none; color: var(--muted); padding: 8px 12px; border-bottom: 2px solid transparent; margin-bottom: -1px; }
  .tabs button.on { color: var(--ink); border-bottom-color: var(--blue); }
  .tabs i { font-style: normal; color: var(--blue); }
  .size { display: flex; gap: 10px; }
  .size button { flex: 1; text-align: left; display: flex; flex-direction: column; gap: 2px; padding: 10px 12px; font-weight: 400; }
  .size button.on { border-color: var(--blue); background: var(--blue-soft); }
  .size span { color: var(--muted); }
  .note { color: var(--muted); margin: 0; }
  footer { margin-top: 8px; display: flex; align-items: center; gap: 12px; }
  footer > span { margin-right: auto; color: var(--muted); }
  .link { border: none; background: none; color: var(--blue); padding: 0; text-decoration: underline; font-weight: 400; }
</style>
