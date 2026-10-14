// Opens the presenter window, and with two screens puts the slides on the
// other one (the projector), like PowerPoint does.
import { getCurrentWindow, availableMonitors, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export const PRESENTER_LABEL = "presenter";

/** Where the main window was before it moved to the projector, to put it back afterwards. */
let before: { x: number; y: number } | null = null;

export async function openPresenter(): Promise<void> {
  if (await WebviewWindow.getByLabel(PRESENTER_LABEL)) return;
  const here = await currentMonitor().catch(() => null);
  const scale = here?.scaleFactor ?? 1;
  const at = here ? { x: here.position.x / scale + 40, y: here.position.y / scale + 40 } : {};
  const win = new WebviewWindow(PRESENTER_LABEL, {
    url: "index.html?presenter=1",
    title: "Lemmata presenter view",
    width: 1180,
    height: 740,
    ...at,
  });
  await new Promise<void>((resolve) => {
    void win.once("tauri://created", () => resolve());
    void win.once("tauri://error", () => resolve());
  });
}

export async function closePresenter(): Promise<void> {
  await (await WebviewWindow.getByLabel(PRESENTER_LABEL))?.close().catch(() => {});
}

/** True when a second screen exists. */
export async function hasSecondScreen(): Promise<boolean> {
  return ((await availableMonitors().catch(() => [])) ?? []).length > 1;
}

/** Moves the main window to a screen that is not the one it is on, so the audience sees the slides there. */
export async function moveToProjector(): Promise<boolean> {
  const win = getCurrentWindow();
  const [monitors, here] = await Promise.all([availableMonitors().catch(() => []), currentMonitor().catch(() => null)]);
  const other = (monitors ?? []).find((m) => !here || m.position.x !== here.position.x || m.position.y !== here.position.y);
  if (!other) return false;
  before = await win.outerPosition().then((p) => ({ x: p.x, y: p.y })).catch(() => null);
  await win.setPosition(new PhysicalPosition(other.position.x + 20, other.position.y + 20));
  return true;
}

export async function moveBack(): Promise<void> {
  if (!before) return;
  await getCurrentWindow().setPosition(new PhysicalPosition(before.x, before.y)).catch(() => {});
  before = null;
}
