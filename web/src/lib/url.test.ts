import { describe, expect, it } from "vitest";
import { joinUrl, pathParamNames, splitUrl, syncPathParams } from "./url";

describe("splitUrl", () => {
  it("splits base, parameters and hash", () => {
    expect(splitUrl("https://x.test/a?b=1&c&d=e=f#top")).toEqual({
      base: "https://x.test/a",
      params: [
        { name: "b", value: "1", enabled: true },
        { name: "c", value: "", enabled: true },
        { name: "d", value: "e=f", enabled: true },
      ],
      hash: "#top",
    });
  });

  it("ignores question marks inside variables", () => {
    expect(splitUrl("{{base?}}/x?q={{a?b}}").base).toBe("{{base?}}/x");
    expect(splitUrl("{{base?}}/x?q={{a?b}}").params[0]?.value).toBe("{{a?b}}");
  });

  it("round-trips through joinUrl", () => {
    const url = "{{baseUrl}}/users?page=2&sort={{order}}";
    const { base, params, hash } = splitUrl(url);
    expect(joinUrl(base, params, hash)).toBe(url);
  });
});

describe("joinUrl", () => {
  it("leaves out disabled and empty parameters", () => {
    expect(
      joinUrl("/a", [
        { name: "x", value: "1" },
        { name: "y", value: "2", enabled: false },
        { name: "", value: "" },
      ]),
    ).toBe("/a?x=1");
  });
});

describe("path parameters", () => {
  it("finds :names in the path but not the port", () => {
    expect(pathParamNames("http://localhost:8080/users/:id/posts/:postId?x=:y")).toEqual([
      "id",
      "postId",
    ]);
    expect(pathParamNames("{{base}}/a/:id")).toEqual(["id"]);
  });

  it("keeps values for names that remain", () => {
    expect(syncPathParams("/u/:id/:tab", { id: "7", old: "x" })).toEqual({ id: "7", tab: "" });
  });
});
