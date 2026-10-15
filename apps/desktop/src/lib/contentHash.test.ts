import { describe, expect, it } from "vitest";
import { contentHash } from "./contentHash";

describe("contentHash", () => {
  // The same vectors are checked in src-tauri/src/files.rs.
  it("matches FNV-1a", () => {
    expect(contentHash("")).toBe(0x811c9dc5);
    expect(contentHash("hello")).toBe(0x4f9f2cab);
    expect(contentHash("Maîtrise ∑")).toBe(contentHash("Maîtrise ∑"));
    expect(contentHash("a")).not.toBe(contentHash("b"));
  });
});
