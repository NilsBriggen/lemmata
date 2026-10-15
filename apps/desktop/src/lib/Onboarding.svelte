<script lang="ts">
  // Shown when no usable TeX installation was found. Lemmata never runs
  // the install itself: it shows the command for this system and checks again.
  import { open } from "@tauri-apps/plugin-dialog";
  import { untrack } from "svelte";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import { updateSettings } from "./settings.svelte";
  import type { TexStatus } from "./api";
  import CommandBox from "./CommandBox.svelte";

  interface Props {
    status: TexStatus;
    checking: boolean;
    onRecheck: (extraDir?: string) => void;
    /** Opens the app anyway: projects can be edited, nothing compiles until TeX is found. */
    onSkip?: () => void;
  }
  let { status, checking, onRecheck, onSkip }: Props = $props();

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
    if (typeof dir === "string") {
      await updateSettings({ texDir: dir });
      onRecheck(dir);
    }
  }
</script>

<div class="onb">
  <div class="top"><span class="brand">Lemmata</span></div>
  <main class="card">
    <span class="eyebrow">One step before you start</span>
    <h1>{found === 0 ? "TeX Live isn't installed yet" : "TeX Live is incomplete"}</h1>
    <p class="lede">
      Lemmata uses the TeX installation on your computer to build PDFs. We looked in your PATH and the usual install
      folders{found === 0 ? " and found nothing" : ", but some required tools are missing"}. Install it with the command below,
      then click Check again.
    </p>

    <div class="checks">
      {#each status.detection.tools as t (t.name)}
        <div class="chk" class:yes={t.path} class:no={!t.path} class:optional={!t.required}>
          <b>{#if t.path}<CircleCheck size={14} />{:else}<CircleX size={14} />{/if} {t.name}</b>
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
      {#if onSkip}<button class="btn lg" onclick={onSkip}>Continue without compiling</button>{/if}
      <button class="btn primary lg" disabled={checking} onclick={() => onRecheck()}><span class:spin={checking}><RefreshCw size={16} /></span> {checking ? "Checking…" : "Check again"}</button>
    </footer>
  </main>
</div>

<style>
  .onb {
    height: 100%; overflow: auto; display: flex; flex-direction: column; align-items: center; padding: 24px 24px 40px;
    background: radial-gradient(1200px 500px at 50% -10%, color-mix(in srgb, var(--accent) 12%, transparent), transparent 70%), var(--app);
  }
  .top { width: min(860px, 100%); margin-bottom: 5vh; }
  .brand { font: 22px var(--f-display); }
  .card {
    width: min(860px, 100%); background: var(--surface); border: 1px solid var(--border); border-radius: 14px;
    box-shadow: var(--shadow); padding: 32px 36px 26px; display: flex; flex-direction: column; gap: 16px;
  }
  .eyebrow { font: 600 11px var(--f-body); letter-spacing: 0.08em; text-transform: uppercase; color: var(--accent); }
  h1 { font: 400 30px var(--f-display); margin: -6px 0 0; }
  .lede { color: var(--muted); max-width: 64ch; margin: 0; font-size: 14.5px; line-height: 1.55; }
  .checks { display: grid; grid-template-columns: repeat(auto-fill, minmax(100px, 1fr)); gap: 8px; }
  .chk { border: 1px solid var(--border); border-radius: var(--r); padding: 8px 10px; display: flex; flex-direction: column; gap: 2px; background: var(--surface-2); }
  .chk b { display: flex; align-items: center; gap: 6px; font: 500 12.5px var(--f-mono); color: var(--ink); }
  .chk span { color: var(--muted); font-size: 11.5px; }
  .chk.yes b :global(svg) { color: var(--ok); }
  .chk.no:not(.optional) { border-color: color-mix(in srgb, var(--error) 40%, var(--border)); background: var(--error-soft); }
  .chk.no:not(.optional) b :global(svg) { color: var(--error); }
  .chk.no.optional b :global(svg) { color: var(--faint); }
  .tabs { display: flex; flex-wrap: wrap; border-bottom: 1px solid var(--border); }
  .tabs button { border: none; background: none; color: var(--muted); height: 36px; padding: 0 12px; border-bottom: 2px solid transparent; margin-bottom: -1px; font: 500 13px var(--f-body); }
  .tabs button.on { color: var(--ink); border-bottom-color: var(--accent); }
  .tabs i { font-style: normal; color: var(--accent); }
  .size { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .size button { text-align: left; display: flex; flex-direction: column; gap: 2px; padding: 12px 14px; border-radius: var(--r-lg); border: 1px solid var(--border); background: var(--surface); color: var(--ink); }
  .size button.on { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent); }
  .size span { color: var(--muted); font-size: 12.5px; }
  .note { color: var(--muted); margin: 0; }
  footer { margin-top: 6px; padding-top: 16px; border-top: 1px solid var(--border); display: flex; align-items: center; gap: 12px; }
  footer > span { margin-right: auto; color: var(--muted); }
  footer .btn span { display: inline-flex; }
  .link { border: none; background: none; color: var(--accent); padding: 0; text-decoration: underline; text-underline-offset: 3px; }
</style>
