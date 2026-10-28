import { describe, expect, it } from "vitest";
import { formatCheck, parseCheck } from "./checks";

describe("checks", () => {
  it("parses target, operator and value", () => {
    expect(parseCheck("status == 200")).toEqual({ target: "status", operator: "==", value: "200" });
    expect(parseCheck("header.content-type contains json")).toEqual({
      target: "header.content-type",
      operator: "contains",
      value: "json",
    });
    expect(parseCheck("json.id exists")).toEqual({
      target: "json.id",
      operator: "exists",
      value: "",
    });
  });

  it("reads operators written with several words", () => {
    expect(parseCheck("status not in 400, 404")).toEqual({
      target: "status",
      operator: "not in",
      value: "400, 404",
    });
    expect(parseCheck("time between 1 and 9")).toEqual({
      target: "time",
      operator: "between",
      value: "1 and 9",
    });
    expect(parseCheck("json.items is not empty")).toEqual({
      target: "json.items",
      operator: "is not empty",
      value: "",
    });
    expect(parseCheck("json.ok is truthy").operator).toBe("is truthy");
    expect(parseCheck("json.id is string")).toEqual({
      target: "json.id",
      operator: "is",
      value: "string",
    });
  });

  it("formats the new operators", () => {
    expect(formatCheck({ target: "json.items", operator: "is empty", value: "x" })).toBe(
      "json.items is empty",
    );
    expect(formatCheck({ target: "status", operator: "in", value: "200, 201" })).toBe(
      "status in 200, 201",
    );
  });

  it("keeps spaces inside quoted keys", () => {
    expect(parseCheck('json["a b"] == 1').target).toBe('json["a b"]');
  });

  it("splits symbols written without spaces", () => {
    expect(parseCheck("status==201")).toEqual({ target: "status", operator: "==", value: "201" });
  });

  it("formats rows back into lines", () => {
    expect(formatCheck({ target: "json.id", operator: "exists", value: "ignored" })).toBe(
      "json.id exists",
    );
    expect(formatCheck({ target: "time", operator: "<", value: " 500 " })).toBe("time < 500");
  });
});
