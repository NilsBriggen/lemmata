// Smoke test of the main loop: open a project, the PDF builds, an edit
// rebuilds it, a missing package shows its install command, typing an
// environment completes its \end, and a click in the PDF finds the source.
import { browser, $, expect } from "@wdio/globals";
import path from "node:path";
import { execSync } from "node:child_process";

const fixtures = path.resolve(import.meta.dirname, "../../../../tests/fixtures");

async function openProject(dir: string) {
  // A build still running when the page reloads is cancelled; let it finish first.
  await browser.waitUntil(async () => !(await $("button.compile, .compile .main").getText().catch(() => "")).includes("Compiling"), { timeout: 60_000 }).catch(() => {});
  // The app reopens the most recent project on start.
  await browser.execute((d) => localStorage.setItem("lemmata.recent", JSON.stringify([d])), dir);
  await browser.refresh();
  await $(".cm-content").waitForExist();
}

describe("Lemmata", () => {
  // The spec reporter only prints at the end, so say where we are as we go.
  const processes = () => {
    try {
      return execSync("ps -eo pid,pgid,ppid,etimes,args --sort=pid | grep -E 'latexmk|pdflatex|biber|lemmata-app|wdio|tauri-driver|WebKit' | grep -v grep", { encoding: "utf8" });
    } catch {
      return "(none)";
    }
  };
  beforeEach(function () {
    console.log(`[e2e] start: ${this.currentTest?.title}\n${processes()}`);
  });
  afterEach(function () {
    console.log(`[e2e] ${this.currentTest?.state}: ${this.currentTest?.title}`);
  });

  it("builds a project and shows its PDF", async () => {
    await openProject(path.join(fixtures, "article"));
    await expect($(".cm-content")).toHaveText(expect.stringContaining("\\documentclass"));
    await $(".pages .page canvas").waitForExist({ timeout: 90_000 });
    await expect($(".status .ok")).toHaveText(expect.stringContaining("Compiled in"));
  });

  it("rebuilds after an edit and lists problems", async () => {
    await $(".cm-content").click();
    // At the very top: text after \end{document} would be ignored by LaTeX.
    await browser.keys(["Control", "Home"]);
    await browser.keys(["Control"]); // release
    await browser.keys("\\undefinedcommandxyz");
    await browser.keys("Escape"); // close completion so Enter adds a line
    await browser.keys("Enter");
    // The error shows in the Logs view (a PDF still exists, so it doesn't open by itself).
    await $(".logs .badge.error").waitForExist({ timeout: 60_000 });
    if ((await $("button.logs").getAttribute("aria-pressed")) !== "true") await $("button.logs").click();
    await expect($(".card.error")).toHaveText(expect.stringContaining("Undefined control sequence"));
    await $(".cm-content").click();
    await browser.keys(["Control", "z"]);
  });

  it("explains a missing package", async () => {
    await openProject(path.join(fixtures, "missing-package"));
    // No PDF, so the logs open by themselves, like Overleaf.
    await $(".card.error").waitForExist({ timeout: 60_000 });
    await expect($(".card.error")).toHaveText(expect.stringContaining("lemmata-does-not-exist.sty"));
  });

  it("completes an environment together with its \\end", async () => {
    await openProject(path.join(fixtures, "article"));
    await $(".cm-content").click();
    await browser.keys(["Control", "Home"]);
    await browser.keys(["Control"]); // release
    await browser.keys("\\begin{itemiz");
    await $(".cm-tooltip-autocomplete").waitForExist({ timeout: 15_000 });
    // CodeMirror ignores Enter for a moment after the list opens.
    await browser.pause(500);
    await browser.keys("Enter");
    await expect($(".cm-content")).toHaveText(expect.stringContaining("\\end{itemize}"));
    // Put the fixture back as it was (autosave writes it to disk).
    for (let i = 0; i < 4; i++) {
      if (!(await $(".cm-content").getText()).includes("itemize")) break;
      await browser.keys(["Control", "z"]);
      await browser.keys(["Control"]);
    }
    await browser.waitUntil(async () => (await $(".status").getText()).includes("Saved"), { timeout: 15_000 });
  });

  it("jumps from the PDF to the source", async () => {
    await openProject(path.join(fixtures, "article"));
    await $(".pages .page canvas").waitForExist({ timeout: 90_000 });
    // On the first page, the text between 31% and 40% of the height comes from chapters/method.tex (checked with `synctex edit`). The point is
    // given as a share of the page, so the click works however the pane is sized.
    await browser.execute(() => {
      const page = document.querySelector(".pages .page") as HTMLElement;
      page.scrollIntoView({ block: "start" });
      const r = page.getBoundingClientRect();
      page.dispatchEvent(
        new MouseEvent("dblclick", { bubbles: true, clientX: r.left + r.width * 0.34, clientY: r.top + r.height * 0.36 }),
      );
    });
    await browser.waitUntil(async () => (await $(".tab.on .name").getText()).includes("method.tex"), {
      timeout: 15_000,
      timeoutMsg: "the editor didn't open the source file",
    });
  });
});
