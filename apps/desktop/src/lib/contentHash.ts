// FNV-1a (32 bit) over the UTF-8 bytes: the same function as content_hash in
// src-tauri/src/files.rs, so a save can say which version it expects on disk.
const encoder = new TextEncoder();

export function contentHash(text: string): number {
  let h = 0x811c9dc5;
  for (const b of encoder.encode(text)) {
    h ^= b;
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}
