// Drag state shared by the nested Tree components: which entry is being
// dragged and which folder it's over. Pointer events are used instead of
// HTML5 drag and drop, which Tauri turns off on Windows.
export const dragState = $state({ active: false, path: "", name: "", x: 0, y: 0, target: null as string | null });

export const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");

/** Can `path` be moved into `folder` ("" is the project root)? */
export function canDrop(path: string, folder: string): boolean {
  if (folder === parentOf(path)) return false;
  return folder !== path && !folder.startsWith(path + "/");
}

let justDragged = false;
/** True right after a drag ends, so the click that follows can be ignored. */
export const wasDrag = () => justDragged;

export function startDrag(
  e: PointerEvent,
  entry: { path: string; name: string },
  onDrop: (path: string, folder: string) => void,
) {
  if (e.button !== 0) return;
  const [sx, sy] = [e.clientX, e.clientY];
  let started = false;
  const move = (ev: PointerEvent) => {
    if (!started && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 6) return;
    started = true;
    const el = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>("[data-drop]");
    const folder = el?.dataset.drop ?? null;
    Object.assign(dragState, {
      active: true,
      path: entry.path,
      name: entry.name,
      x: ev.clientX,
      y: ev.clientY,
      target: folder !== null && canDrop(entry.path, folder) ? folder : null,
    });
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", up);
    if (!started) return;
    const target = dragState.target;
    dragState.active = false;
    dragState.target = null;
    justDragged = true;
    setTimeout(() => (justDragged = false), 60);
    if (target !== null) onDrop(entry.path, target);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
}
