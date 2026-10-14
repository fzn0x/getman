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
