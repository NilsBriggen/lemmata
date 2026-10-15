// Mock of @tauri-apps/api/app for the browser preview (vite --mode mock).
// `?version=2.0.0` pretends to be that version, e.g. to see the "What's new" window.
export async function getVersion() {
  return new URLSearchParams(location.search).get("version") ?? "0.1.0-preview";
}
