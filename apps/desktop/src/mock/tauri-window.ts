// The preview runs in a normal browser tab: there is no window to make full
// screen, and one screen only (so the presenter window opens on request).
export class PhysicalPosition {
  constructor(
    public x: number,
    public y: number,
  ) {}
}
export function getCurrentWindow() {
  return {
    setFullscreen: async () => {},
    isFullscreen: async () => false,
    outerPosition: async () => new PhysicalPosition(0, 0),
    setPosition: async () => {},
    close: async () => window.close(),
  };
}
export const availableMonitors = async () => [{ position: new PhysicalPosition(0, 0), scaleFactor: 1 }];
export const currentMonitor = async () => ({ position: new PhysicalPosition(0, 0), scaleFactor: 1 });
