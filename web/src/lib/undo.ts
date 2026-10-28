// A short history of what was done to the sidebar tree, to undo and redo
// with Ctrl+Z and Ctrl+Shift+Z. The files stay where the actions put them;
// each action knows how to put its change back and to make it again.

export interface Undoable {
  /** Says what was done, such as `rename of A to B`. */
  label: string;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
}

/** Actions kept; the oldest are forgotten. */
export const UNDO_LIMIT = 20;

export class UndoStack {
  private past: Undoable[] = [];
  private future: Undoable[] = [];
  private busy = false;

  constructor(private readonly limit = UNDO_LIMIT) {}

  get canUndo(): boolean {
    return this.past.length > 0;
  }

  get canRedo(): boolean {
    return this.future.length > 0;
  }

  /** Notes a new action. What was undone before it can no longer be redone. */
  record(entry: Undoable): void {
    this.past.push(entry);
    if (this.past.length > this.limit) this.past.splice(0, this.past.length - this.limit);
    this.future = [];
  }

  /**
   * Undoes the latest action. Returns it, or `null` when there is none or
   * another undo or redo is still running. An action that fails is
   * dropped, since the tree is no longer as it expected, and its error
   * is thrown.
   */
  async undo(): Promise<Undoable | null> {
    const entry = this.past.at(-1);
    if (!entry || this.busy) return null;
    this.busy = true;
    this.past.pop();
    try {
      await entry.undo();
      this.future.push(entry);
      return entry;
    } finally {
      this.busy = false;
    }
  }

  /** Makes the action undone last again, in the same way as `undo`. */
  async redo(): Promise<Undoable | null> {
    const entry = this.future.at(-1);
    if (!entry || this.busy) return null;
    this.busy = true;
    this.future.pop();
    try {
      await entry.redo();
      this.past.push(entry);
      return entry;
    } finally {
      this.busy = false;
    }
  }

  clear(): void {
    this.past = [];
    this.future = [];
  }
}
