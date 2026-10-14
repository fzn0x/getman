import { describe, expect, it } from "vitest";
import { fuzzy, rank } from "./fuzzy";

describe("fuzzy", () => {
  it("needs every character in order", () => {
    expect(fuzzy("crt", "create user")).not.toBeNull();
    expect(fuzzy("tcr", "create user")).toBeNull();
  });

  it("ranks word starts and prefixes first", () => {
    const items = ["Delete user", "List users", "users/create-user.yaml", "Update"];
    expect(rank("us", items, (s) => s)[0]).toBe("users/create-user.yaml");
    expect(rank("lu", items, (s) => s)[0]).toBe("List users");
  });
});
