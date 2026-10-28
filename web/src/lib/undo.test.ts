import { describe, expect, it } from "vitest";
import { UndoStack, type Undoable } from "./undo";

function action(log: string[], name: string, fail = false): Undoable {
  return {
    label: name,
    undo: async () => {
      if (fail) throw new Error("gone");
      log.push(`undo ${name}`);
    },
    redo: async () => {
      log.push(`redo ${name}`);
    },
  };
}

describe("UndoStack", () => {
  it("undoes and redoes the latest action first", async () => {
    const log: string[] = [];
    const stack = new UndoStack();
    stack.record(action(log, "a"));
    stack.record(action(log, "b"));
    expect((await stack.undo())?.label).toBe("b");
    expect((await stack.undo())?.label).toBe("a");
    expect(await stack.undo()).toBeNull();
    expect((await stack.redo())?.label).toBe("a");
    expect((await stack.redo())?.label).toBe("b");
    expect(await stack.redo()).toBeNull();
    expect(log).toEqual(["undo b", "undo a", "redo a", "redo b"]);
  });

  it("forgets what could be redone when something new is done", async () => {
    const stack = new UndoStack();
    stack.record(action([], "a"));
    await stack.undo();
    expect(stack.canRedo).toBe(true);
    stack.record(action([], "b"));
    expect(stack.canRedo).toBe(false);
    expect(stack.canUndo).toBe(true);
  });

  it("keeps a few actions", async () => {
    const stack = new UndoStack(3);
    for (const name of ["a", "b", "c", "d"]) stack.record(action([], name));
    const undone: string[] = [];
    for (let entry = await stack.undo(); entry; entry = await stack.undo())
      undone.push(entry.label);
    expect(undone).toEqual(["d", "c", "b"]);
  });

  it("drops an action that fails to undo", async () => {
    const stack = new UndoStack();
    stack.record(action([], "a"));
    stack.record(action([], "b", true));
    await expect(stack.undo()).rejects.toThrow("gone");
    expect(stack.canRedo).toBe(false);
    expect((await stack.undo())?.label).toBe("a");
  });

  it("does not start a second undo while one runs", async () => {
    const stack = new UndoStack();
    let release = () => {};
    const slow: Undoable = {
      label: "slow",
      undo: () => new Promise<void>((resolve) => (release = resolve)),
      redo: async () => {},
    };
    stack.record(action([], "a"));
    stack.record(slow);
    const first = stack.undo();
    expect(await stack.undo()).toBeNull();
    release();
    expect((await first)?.label).toBe("slow");
    expect((await stack.undo())?.label).toBe("a");
  });
});
