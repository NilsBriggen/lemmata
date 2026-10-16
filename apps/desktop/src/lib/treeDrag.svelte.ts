// Drag state shared by the nested Tree components (and the start screen's projects):
// which entries are being dragged and which folder they're over. Pointer events are used instead of
// HTML5 drag and drop, which Tauri turns off on Windows.
import { canMove, parentOf as parent } from "./treeModel";

export const dragState = $state({
  active: false,
  paths: [] as string[],
  /** What the ghost shows: a name, or "3 items". */
  name: "",
  x: 0,
  y: 0,
  target: null as string | null,
  /** A folder an OS file drag is hovering over ("" is the project root). */
  external: null as string | null,
});

export const parentOf = parent;

/** Can `path` be moved into `folder` ("" is the project root)? */
export const canDrop = (path: string, folder: string) => canMove([path], folder);

let justDragged = false;
/** True right after a drag ends, so the click that follows can be ignored. */
export const wasDrag = () => justDragged;

export function startDrag(
  e: PointerEvent,
  entry: { path: string; name: string },
  /** The paths that travel with this row: the selection when it is part of it. */
  paths: () => string[],
  onDrop: (paths: string[], folder: string) => void,
  /** Whether `paths` may be dropped on the `[data-drop]` target named `folder`; the file tree's rule by default. */
  allowed: (paths: string[], folder: string) => boolean = canMove,
) {
  if (e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey) return;
  const [sx, sy] = [e.clientX, e.clientY];
  let started = false;
  let dragged: string[] = [];
  const move = (ev: PointerEvent) => {
    if (!started && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 6) return;
    if (!started) dragged = paths();
    started = true;
    const el = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>("[data-drop]");
    const folder = el?.dataset.drop ?? null;
    Object.assign(dragState, {
      active: true,
      paths: dragged,
      name: dragged.length > 1 ? `${dragged.length} items` : entry.name,
      x: ev.clientX,
      y: ev.clientY,
      target: folder !== null && allowed(dragged, folder) ? folder : null,
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
    if (target !== null) onDrop(dragged, target);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
}
