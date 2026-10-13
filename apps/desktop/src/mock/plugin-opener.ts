export async function openUrl(url: string) {
  window.open(url, "_blank");
}
export async function revealItemInDir(path: string) {
  console.info("[mock] reveal", path);
}
