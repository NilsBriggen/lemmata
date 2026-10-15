// The open files' text and what is known about the disk: unsaved edits,
// autosave timers, conflicts with other programs and failed saves. No UI and
// no Tauri here, so the rules that keep edits safe can be unit-tested; the app
// reacts to the results (builds, outline, git marks).
import { contentHash } from "./contentHash";
import { errorMessage } from "./api";

export interface BufferIO {
  /** Writes `text`; `expected` is the hash of what should be on disk now, and a mismatch throws "conflict: …". */
  write(path: string, text: string, expected: number | undefined): Promise<void>;
  /** Delay after the last edit before saving; 0 saves only when asked. */
  autosaveMs(): number;
  /** The autosave delay passed for `path`. */
  autosave(path: string): void;
}

export type WriteResult = { saved: string } | { conflict: true } | { error: string };

/** What to do about a file that changed on disk. */
export type OutsideChange = "ignore" | "conflict" | "reload";

export class Buffers {
  /** Each open file's text, as in the editor. */
  readonly contents = new Map<string, string>();
  /** What the disk holds, as far as the app knows: last read or written. */
  readonly lastSaved = new Map<string, string>();
  dirty = $state<Record<string, boolean>>({});
  /** Files changed by another program while they had unsaved edits here: the user decides. */
  conflicts = $state<string[]>([]);
  failedSaves = $state<string[]>([]);
  private timers = new Map<string, ReturnType<typeof setTimeout>>();

  constructor(private io: BufferIO) {}

  /** Paths with edits not on disk yet. */
  get unsaved(): string[] {
    return Object.keys(this.dirty).filter((p) => this.dirty[p]);
  }

  /** Text read from disk. */
  loaded(path: string, text: string) {
    this.contents.set(path, text);
    this.lastSaved.set(path, text);
  }

  /** An edit in the editor: unsaved until the autosave delay passes (never while in conflict). */
  edit(path: string, text: string) {
    this.contents.set(path, text);
    this.dirty[path] = true;
    this.schedule(path);
  }

  schedule(path: string) {
    this.cancel(path);
    const ms = this.io.autosaveMs();
    if (ms > 0 && !this.conflicts.includes(path)) this.timers.set(path, setTimeout(() => this.io.autosave(path), ms));
  }

  cancel(path: string) {
    clearTimeout(this.timers.get(path));
    this.timers.delete(path);
  }

  /** No pending autosave runs (e.g. before a restore writes the files). */
  cancelAll() {
    for (const t of this.timers.values()) clearTimeout(t);
    this.timers.clear();
  }

  /**
   * Saves `path` if it has unsaved edits and isn't in conflict; null when there was nothing to do.
   * The disk must still hold what was last read or written, or it's a conflict and nothing is written.
   */
  async write(path: string): Promise<WriteResult | null> {
    this.cancel(path);
    const text = this.contents.get(path);
    if (text === undefined || !this.dirty[path] || this.conflicts.includes(path)) return null;
    const known = this.lastSaved.get(path);
    try {
      await this.io.write(path, text, known === undefined ? undefined : contentHash(known));
    } catch (e) {
      const message = errorMessage(e);
      if (message.startsWith("conflict:")) {
        this.conflict(path);
        return { conflict: true };
      }
      if (!this.failedSaves.includes(path)) this.failedSaves = [...this.failedSaves, path];
      return { error: message };
    }
    this.lastSaved.set(path, text);
    this.failedSaves = this.failedSaves.filter((f) => f !== path);
    // Typing continued while the write was in flight: those edits aren't saved yet.
    if (this.contents.get(path) === text) this.dirty[path] = false;
    else this.schedule(path);
    return { saved: text };
  }

  conflict(path: string) {
    this.cancel(path);
    if (!this.conflicts.includes(path)) this.conflicts = [...this.conflicts, path];
  }

  /** The user picked a side. "mine": the next write goes over the other program's version. "disk": ours is dropped; reload it. */
  resolve(path: string, keep: "mine" | "disk") {
    this.conflicts = this.conflicts.filter((c) => c !== path);
    if (keep === "mine") this.lastSaved.delete(path);
    else this.reset(path);
  }

  /** Drops the app's copy, so the file is read from disk the next time it's shown. */
  reset(path: string) {
    this.cancel(path);
    this.contents.delete(path);
    this.lastSaved.delete(path);
    this.dirty[path] = false;
  }

  /** The file was closed or removed: nothing of it is kept, and no pending save runs. */
  forget(path: string) {
    this.reset(path);
    this.conflicts = this.conflicts.filter((c) => c !== path);
  }

  /** Another program changed `path`; `disk` is its text now. */
  outsideChange(path: string, disk: string): OutsideChange {
    // What we last read or wrote (our own autosave), or what's in the editor, isn't an outside change.
    if (disk === this.lastSaved.get(path)) return "ignore";
    if (disk === this.contents.get(path)) {
      this.lastSaved.set(path, disk);
      return "ignore";
    }
    if (this.dirty[path]) {
      this.conflict(path);
      return "conflict";
    }
    return "reload";
  }

  /** A file or folder moved: unsaved marks and conflicts follow; the text is read again under the new name. */
  rename(from: string, to: string) {
    const inside = (p: string) => p === from || p.startsWith(from + "/");
    const moved = (p: string) => (p === from ? to : inside(p) ? to + p.slice(from.length) : p);
    for (const p of [...this.contents.keys(), ...this.lastSaved.keys(), ...this.timers.keys()].filter(inside)) {
      this.cancel(p);
      this.contents.delete(p);
      this.lastSaved.delete(p);
    }
    this.dirty = Object.fromEntries(Object.entries(this.dirty).map(([k, v]) => [moved(k), v]));
    this.conflicts = this.conflicts.map(moved);
  }

  /** The project closed. */
  clear() {
    this.cancelAll();
    this.contents.clear();
    this.lastSaved.clear();
    this.dirty = {};
    this.conflicts = [];
    this.failedSaves = [];
  }
}
