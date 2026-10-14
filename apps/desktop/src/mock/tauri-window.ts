// The preview runs in a normal browser tab: there is no window to make full screen.
export function getCurrentWindow() {
  return { setFullscreen: async () => {}, isFullscreen: async () => false };
}
