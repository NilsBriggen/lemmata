<script lang="ts">
  // Shown once after an update: what people can do now that they couldn't before.
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Modal from "./ui/Modal.svelte";
  import type { Release } from "./whatsNew";

  let { version, releases, onClose }: { version: string; releases: Release[]; onClose: () => void } = $props();
  // Updating across several releases: label each part, newest first.
  const labelled = $derived(releases.length > 1);
</script>

<Modal title="What's new in Lemmata" subtitle="You're now on version {version}." width={500} {onClose}>
  <div class="news">
    {#each releases as r (r.version)}
      <section>
        {#if labelled}<h3>Lemmata {r.version.replace(/\.0$/, "")}</h3>{/if}
        <ul>
          {#each r.highlights as h (h.title)}
            <li>
              <Sparkles size={15} />
              <div><b>{h.title}</b><span>{h.text}</span></div>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  </div>
  {#snippet footer()}
    <button class="btn primary" onclick={onClose}>Got it</button>
  {/snippet}
</Modal>

<style>
  .news { display: flex; flex-direction: column; gap: 14px; max-height: min(460px, 60vh); overflow-y: auto; }
  h3 { margin: 0 0 6px; font-size: 12px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 10px; }
  li { display: flex; gap: 10px; align-items: flex-start; }
  li :global(svg) { flex: none; margin-top: 2px; color: var(--accent); }
  li div { display: flex; flex-direction: column; gap: 1px; }
  li span { color: var(--muted); }
</style>
