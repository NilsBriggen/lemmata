import { describe, expect, it } from "vitest";
import { clampPage, fitScale, formatElapsed, hasNotesPage, regionSize, regions } from "./layout";

describe("notes pages", () => {
  it("tells slides from slides with notes beside them", () => {
    expect(hasNotesPage(455, 256)).toBe(false); // 16:9
    expect(hasNotesPage(362, 272)).toBe(false); // 4:3
    expect(hasNotesPage(910, 256)).toBe(true); // 16:9 slide + notes
    expect(hasNotesPage(724, 272)).toBe(true); // 4:3 slide + notes
    expect(hasNotesPage(595, 842)).toBe(false); // a portrait document
    expect(hasNotesPage(100, 0)).toBe(false);
  });

  it("puts the slide on the left and the notes on the right", () => {
    expect(regions(910, 256)).toEqual({ slide: "left", notes: "right" });
    expect(regions(455, 256)).toEqual({ slide: "full", notes: null });
    expect(regionSize(910, 256, "left")).toEqual({ w: 455, h: 256, x: 0 });
    expect(regionSize(910, 256, "right")).toEqual({ w: 455, h: 256, x: 455 });
    expect(regionSize(455, 256, "full")).toEqual({ w: 455, h: 256, x: 0 });
  });
});

describe("helpers", () => {
  it("fits a region into a box", () => {
    expect(fitScale({ w: 400, h: 200 }, 800, 800)).toBe(2);
    expect(fitScale({ w: 400, h: 200 }, 800, 200)).toBe(1);
    expect(fitScale({ w: 0, h: 0 }, 800, 600)).toBe(1);
  });
  it("formats the timer", () => {
    expect(formatElapsed(0)).toBe("0:00");
    expect(formatElapsed(83)).toBe("1:23");
    expect(formatElapsed(3725)).toBe("1:02:05");
    expect(formatElapsed(-4)).toBe("0:00");
  });
  it("keeps the page in range", () => {
    expect(clampPage(0, 10)).toBe(1);
    expect(clampPage(11, 10)).toBe(10);
    expect(clampPage(4.6, 10)).toBe(5);
    expect(clampPage(Number.NaN, 10)).toBe(1);
    expect(clampPage(3, 0)).toBe(1);
  });
});
