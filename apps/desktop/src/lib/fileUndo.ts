// An undo log for file operations in the tree. Each entry knows how to put
// things back; the log holds the last few.
export interface UndoStep {
  label: string;
  undo: () => Promise<void>;
}

const MAX = 20;

export class UndoLog {
  private steps: UndoStep[] = [];

  push(step: UndoStep) {
    this.steps.push(step);
    if (this.steps.length > MAX) this.steps.shift();
  }

  get last(): UndoStep | undefined {
    return this.steps.at(-1);
  }

  get size() {
    return this.steps.length;
  }

  /** Runs the newest step. A step that fails is dropped so Undo never gets stuck on it. */
  async undo(): Promise<UndoStep | null> {
    const step = this.steps.pop();
    if (!step) return null;
    await step.undo();
    return step;
  }

  /** Forget everything, e.g. when another project opens. */
  clear() {
    this.steps = [];
  }
}
