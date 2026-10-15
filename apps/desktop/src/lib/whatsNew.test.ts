import { describe, expect, it } from "vitest";
import { compareVersions, releasesSince } from "./whatsNew";

const versions = (last: string, current: string) => releasesSince(last, current).map((r) => r.version);

describe("what's new", () => {
  it("orders versions, a pre-release before its release", () => {
    expect(compareVersions("1.5.2", "1.10.0")).toBe(-1);
    expect(compareVersions("2.0.0", "2.0.0")).toBe(0);
    expect(compareVersions("2.0.0-rc.2", "2.0.0")).toBe(-1);
    expect(compareVersions("2.0.0-rc.10", "2.0.0-rc.9")).toBe(1);
  });

  it("shows every release skipped, and nothing for a patch", () => {
    expect(versions("1.4.0", "2.0.0")).toEqual(["2.0.0", "1.5.0"]);
    expect(versions("1.5.2", "2.0.0")).toEqual(["2.0.0"]);
    expect(versions("2.0.0", "2.0.1")).toEqual([]);
    expect(versions("2.0.0", "2.0.0")).toEqual([]);
  });

  it("shows a release's news at its first pre-release, once", () => {
    expect(versions("1.5.2", "2.0.0-rc.1")).toEqual(["2.0.0"]);
    expect(versions("2.0.0-rc.1", "2.0.0-rc.2")).toEqual([]);
    expect(versions("2.0.0-rc.2", "2.0.0")).toEqual([]);
  });
});
