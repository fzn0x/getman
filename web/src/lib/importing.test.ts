import { describe, expect, it } from "vitest";
import { describeFiles, describeText, isCollection } from "./importing";

describe("describeText", () => {
  it("names each format", () => {
    const cases: [string, string, string][] = [
      ["curl https://x.test", "", "cURL"],
      ["openapi: 3.1.0\ninfo: {}", "", "OpenAPI"],
      ['{"openapi": "3.0.0"}', "", "OpenAPI"],
      ['{"swagger": "2.0"}', "", "Swagger"],
      ["type: collection.insomnia.rest/5.0\n", "", "Insomnia v5"],
      ['{"_type": "export", "resources": []}', "", "Insomnia v4"],
      ['{"log": {"entries": []}}', "", "HAR"],
      ['{"info": {}, "item": []}', "", "Postman collection"],
      ['{"name": "e", "values": []}', "", "Postman environment"],
      ['{"_postman_variable_scope": "globals", "values": []}', "", "Postman globals"],
      ['{"folders": [], "requests": []}', "", "Hoppscotch collection"],
      ['[{"name": "e", "variables": []}]', "", "Hoppscotch environments"],
      ["meta {\n  name: x\n}", "", "Bruno"],
      ["anything", "get.bru", "Bruno"],
      ['{"a": 1}', "", "does not import"],
    ];
    for (const [text, name, expected] of cases) {
      expect(describeText(text, name), text).toContain(expected);
    }
    expect(describeText("   ")).toBe("");
  });

  it("tells which formats can become the workspace", () => {
    expect(isCollection(describeText('{"info": {}, "item": []}'))).toBe(true);
    expect(isCollection(describeText("openapi: 3.1.0"))).toBe(true);
    expect(isCollection(describeText('{"name": "e", "values": []}'))).toBe(false);
    expect(isCollection(describeText("curl x"))).toBe(false);
  });
});

describe("describeFiles", () => {
  it("recognizes a Bruno folder", () => {
    const files = [
      { path: "shop/bruno.json", text: "{}" },
      { path: "shop/get.bru", text: "meta {\n}" },
    ];
    expect(describeFiles(files)).toBe("A Bruno collection: 2 files");
    expect(
      describeFiles([
        { path: "shop/bruno.json", text: "{}" },
        { path: "x.json", text: "{}" },
      ]),
    ).toContain("Bruno");
    expect(
      describeFiles([
        { path: "a.json", text: "{}" },
        { path: "b.har", text: "{}" },
      ]),
    ).toBe("2 files, each imported in its own format");
  });
});
