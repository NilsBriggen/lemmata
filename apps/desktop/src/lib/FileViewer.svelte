<script lang="ts">
  // Read-only viewer for images and PDFs that are part of the project.
  import PdfView from "./PdfView.svelte";
  import { api, errorMessage } from "./api";

  let { path }: { path: string } = $props();

  const MIME: Record<string, string> = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp", svg: "image/svg+xml" };
  const ext = $derived(path.split(".").pop()!.toLowerCase());
  const isPdf = $derived(ext === "pdf");

  let url = $state<string | null>(null);
  let error = $state<string | null>(null);
  let natural = $state<{ w: number; h: number } | null>(null);
  let actual = $state(false);
  let pdf = $state<PdfView>();

  // Depends on `path` only: reading `url` here would re-run it whenever the image arrives,
  // dropping that image and loading it again, forever.
  $effect(() => {
    const current = path;
    let made: string | null = null;
    url = null;
    error = null;
    natural = null;
    actual = false;
    api
      .readBinary(current)
      .then(async (bytes) => {
        if (current !== path) return;
        if (isPdf) await pdf?.load(bytes);
        else url = made = URL.createObjectURL(new Blob([bytes.buffer as ArrayBuffer], { type: MIME[ext] ?? "application/octet-stream" }));
      })
      .catch((e) => (error = errorMessage(e)));
    return () => made && URL.revokeObjectURL(made);
  });
</script>

<div class="viewer">
  {#if error}
    <p class="msg">{error}</p>
  {:else if isPdf}
    <PdfView bind:this={pdf} onSourceClick={() => {}} />
  {:else if url}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <div class="stage" class:actual>
      <img
        src={url}
        alt={path}
        class:actual
        onload={(e) => {
          const img = e.currentTarget as HTMLImageElement;
          natural = { w: img.naturalWidth, h: img.naturalHeight };
        }}
        onclick={() => (actual = !actual)}
      />
    </div>
    {#if natural}<div class="info">{path.split("/").pop()} · {natural.w} × {natural.h} px · click to {actual ? "fit" : "view at 100%"}</div>{/if}
  {:else}
    <p class="msg">Loading…</p>
  {/if}
</div>

<style>
  .viewer { position: relative; height: 100%; background: var(--pdf-ground); overflow: hidden; }
  .stage { height: 100%; overflow: auto; display: grid; place-items: center; padding: 24px; box-sizing: border-box; }
  .stage.actual { place-items: start; }
  img {
    max-width: 100%; max-height: 100%; object-fit: contain; cursor: zoom-in; background: #fff;
    background-image: conic-gradient(#ececec 25%, #fff 0 50%, #ececec 0 75%, #fff 0); background-size: 16px 16px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18), 0 6px 18px -6px rgba(0, 0, 0, 0.25);
  }
  img.actual { max-width: none; max-height: none; cursor: zoom-out; }
  .info { position: absolute; left: 12px; bottom: 10px; padding: 3px 9px; border-radius: var(--r-sm); background: var(--surface); border: 1px solid var(--border); color: var(--muted); font-size: 12px; }
  .msg { margin: 0; padding: 24px; color: var(--muted); }
</style>
