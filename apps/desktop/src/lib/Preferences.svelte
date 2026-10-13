<script lang="ts">
  // App-wide preferences (lemmata.toml holds the per-project ones).
  import { open } from "@tauri-apps/plugin-dialog";
  import { getVersion } from "@tauri-apps/api/app";
  import { settings, updateSettings, type Settings } from "./settings.svelte";
  import type { Detection } from "./api";

  interface Props {
    detection: Detection | null;
    onTexDir: (dir: string | null) => void;
    onCheckUpdates: () => Promise<string>;
    onClose: () => void;
  }
  let { detection, onTexDir, onCheckUpdates, onClose }: Props = $props();

  let version = $state("");
  let updateMessage = $state<string | null>(null);
  let checking = $state(false);
  getVersion().then((v) => (version = v)).catch(() => {});

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    updateSettings({ [key]: value } as Partial<Settings>);
  }

  async function chooseTex() {
    const dir = await open({ directory: true, title: "Choose the folder that contains pdflatex and latexmk" });
    if (typeof dir === "string") {
      await updateSettings({ texDir: dir });
      onTexDir(dir);
    }
  }

  async function checkNow() {
    checking = true;
    updateMessage = await onCheckUpdates();
    checking = false;
  }

  const distribution = $derived.by(() => {
    const d = detection?.distribution;
    if (!d || d.name === "unknown") return "Unknown TeX distribution";
    return d.name === "texlive" ? `TeX Live ${d.year ?? ""}` : `MiKTeX ${d.version ?? ""}`;
  });
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dialog" role="dialog" aria-labelledby="prefs-title">
    <h2 id="prefs-title">Preferences</h2>

    <section>
      <h3>Appearance</h3>
      <div class="field">
        <label for="pref-theme">Theme</label>
        <select id="pref-theme" value={settings.theme} onchange={(e) => set("theme", e.currentTarget.value as Settings["theme"])}>
          <option value="system">Same as the system</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </div>
      <div class="field">
        <label for="pref-font">Editor text size</label>
        <select id="pref-font" value={String(settings.fontSize)} onchange={(e) => set("fontSize", Number(e.currentTarget.value))}>
          {#each [11, 12, 13, 14, 15, 16, 18, 20] as n (n)}<option value={String(n)}>{n} px</option>{/each}
        </select>
      </div>
      <label class="check" for="pref-pdfdark">
        <input id="pref-pdfdark" type="checkbox" checked={settings.pdfDark} onchange={(e) => set("pdfDark", e.currentTarget.checked)} />
        Show the PDF with dark pages in dark mode
      </label>
    </section>

    <section>
      <h3>Editing</h3>
      <div class="field">
        <label for="pref-save">Save and build</label>
        <select id="pref-save" value={String(settings.autosaveMs)} onchange={(e) => set("autosaveMs", Number(e.currentTarget.value))}>
          <option value="300">Right after you stop typing (0.3 s)</option>
          <option value="600">Shortly after you stop typing (0.6 s)</option>
          <option value="1500">After a pause (1.5 s)</option>
          <option value="0">Only when I press Ctrl+S</option>
        </select>
      </div>
      <div class="field">
        <label for="pref-keymap">Keyboard</label>
        <select id="pref-keymap" value={settings.keymap} onchange={(e) => set("keymap", e.currentTarget.value as Settings["keymap"])}>
          <option value="default">Standard</option>
          <option value="vim">Vim</option>
        </select>
      </div>
    </section>

    <section>
      <h3>TeX</h3>
      <p class="muted">{distribution}{detection?.bin_dir ? ` in ${detection.bin_dir}` : ""}</p>
      <div class="row">
        <button onclick={chooseTex}>Choose TeX folder…</button>
        {#if settings.texDir}
          <button onclick={async () => { await updateSettings({ texDir: null }); onTexDir(null); }}>Find automatically</button>
        {/if}
      </div>
    </section>

    <section>
      <h3>Updates</h3>
      <label class="check" for="pref-updates">
        <input id="pref-updates" type="checkbox" checked={settings.checkForUpdates} onchange={(e) => set("checkForUpdates", e.currentTarget.checked)} />
        Check for updates automatically
      </label>
      <div class="field">
        <label for="pref-channel">Channel</label>
        <select id="pref-channel" value={settings.updateChannel} onchange={(e) => set("updateChannel", e.currentTarget.value as Settings["updateChannel"])}>
          <option value="stable">Stable releases</option>
          <option value="beta">Betas: new features sooner, rougher edges</option>
        </select>
      </div>
      <div class="row">
        <button disabled={checking} onclick={checkNow}>{checking ? "Checking…" : "Check now"}</button>
        {#if updateMessage}<span class="muted">{updateMessage}</span>{/if}
      </div>
    </section>

    <footer>
      <span class="muted">Lemmata {version}</span>
      <button class="primary" onclick={onClose}>Done</button>
    </footer>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(10, 15, 20, 0.45); display: grid; place-items: center; padding: 24px; z-index: 10; }
  .dialog {
    width: min(560px, 100%); max-height: 100%; overflow: auto; background: var(--sheet); border: 1px solid var(--rule);
    border-radius: 8px; padding: 22px 24px; display: flex; flex-direction: column; gap: 18px;
  }
  h2 { font: 400 22px var(--f-display); margin: 0; }
  h3 { font: 500 11px var(--f-mono); letter-spacing: 0.1em; text-transform: uppercase; color: var(--muted); margin: 0 0 8px; }
  section { display: flex; flex-direction: column; gap: 8px; }
  .field { display: grid; grid-template-columns: 150px 1fr; align-items: center; gap: 10px; }
  .field label { color: var(--ink); }
  select { padding: 5px 8px; font-size: 13px; }
  .check { display: flex; gap: 8px; align-items: center; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .muted { color: var(--muted); margin: 0; }
  footer { display: flex; align-items: center; justify-content: space-between; border-top: 1px solid var(--rule); padding-top: 14px; }
</style>
