// Smoke test of the main loop: open a project, the PDF builds, an edit
// rebuilds it, and a missing package shows its install command.
import { browser, $, expect } from "@wdio/globals";
import path from "node:path";

const fixtures = path.resolve(import.meta.dirname, "../../../../tests/fixtures");

async function openProject(dir: string) {
  // The app reopens the most recent project on start.
  await browser.execute((d) => localStorage.setItem("lemmata.recent", JSON.stringify([d])), dir);
  await browser.refresh();
  await $(".cm-content").waitForExist();
}

describe("Lemmata", () => {
  it("builds a project and shows its PDF", async () => {
    await openProject(path.join(fixtures, "article"));
    await expect($(".cm-content")).toHaveText(expect.stringContaining("\\documentclass"));
    await $(".pages .page canvas").waitForExist({ timeout: 90_000 });
    await expect($("header .pill.ok")).toHaveText(expect.stringContaining("built in"));
  });

  it("rebuilds after an edit and lists problems", async () => {
    await $(".cm-content").click();
    // At the very top: text after \end{document} would be ignored by LaTeX.
    await browser.keys(["Control", "Home"]);
    await browser.keys(["Control"]); // release
    await browser.keys("\\undefinedcommandxyz");
    await browser.keys("Escape"); // close completion so Enter adds a line
    await browser.keys("Enter");
    await $(".problems .row.error").waitForExist({ timeout: 60_000 });
    await expect($(".problems")).toHaveText(expect.stringContaining("Undefined control sequence"));
    await browser.keys(["Control", "z"]);
  });

  it("explains a missing package", async () => {
    await openProject(path.join(fixtures, "missing-package"));
    await $(".problems .row.error").waitForExist({ timeout: 60_000 });
    await expect($(".problems")).toHaveText(expect.stringContaining("lemmata-does-not-exist.sty"));
  });
});
