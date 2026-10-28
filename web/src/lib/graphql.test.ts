import { buildSchema, introspectionFromSchema } from "graphql";
import { describe, expect, it } from "vitest";
import {
  describeType,
  diagnose,
  maskVariables,
  namedTypeOf,
  operationNames,
  rootTypes,
  schemaFrom,
  searchDocs,
  typeCount,
} from "./graphql";

const sdl = `
  "Who can sign in."
  type User implements Node {
    id: ID!
    name: String
    "Orders, newest first."
    orders(first: Int = 10, status: Status): [Order!]!
    old: String @deprecated(reason: "Use name.")
  }
  interface Node { id: ID! }
  type Order implements Node { id: ID! total: Float! }
  union Result = User | Order
  enum Status { OPEN SHIPPED }
  input Filter { text: String }
  type Query {
    me: User
    user(id: ID!): User
    search(filter: Filter): [Result!]!
  }
  type Mutation { rename(id: ID!, name: String!): User }
`;

// What a server's introspection answer holds, built back into a schema.
const schema = schemaFrom(introspectionFromSchema(buildSchema(sdl)));

describe("operationNames", () => {
  it("lists named operations", () => {
    expect(
      operationNames('query A { me { id } } mutation B { rename(id: 1, name: "x") { id } }'),
    ).toEqual(["A", "B"]);
    expect(operationNames("{ me { id } }")).toEqual([]);
  });

  it("reads half-typed queries and variables", () => {
    expect(operationNames("query One { user(id: {{id}}) { id } } query Two {")).toEqual([
      "One",
      "Two",
    ]);
  });
});

describe("maskVariables", () => {
  it("keeps offsets", () => {
    const { masked, ranges } = maskVariables("{ user(id: {{userId}}) }");
    expect(masked).toBe("{ user(id: __________) }");
    expect(ranges).toEqual([[11, 21]]);
  });
});

describe("diagnose", () => {
  it("finds syntax errors without a schema", () => {
    const problems = diagnose("query { me { id }", null);
    expect(problems.length).toBeGreaterThan(0);
    expect(problems[0]?.severity).toBe("error");
  });

  it("finds unknown fields with a schema", () => {
    const text = "query { me { id nope } }";
    const problems = diagnose(text, schema);
    expect(problems).toHaveLength(1);
    expect(problems[0]?.message).toContain('Cannot query field "nope"');
    expect(text.slice(problems[0]!.from, problems[0]!.to)).toContain("nope");
  });

  it("accepts a valid query", () => {
    expect(diagnose("query Q { user(id: 1) { name orders(first: 2) { total } } }", schema)).toEqual(
      [],
    );
    expect(diagnose("   ", schema)).toEqual([]);
  });

  it("leaves {{variables}} alone", () => {
    expect(diagnose("query { user(id: {{userId}}) { name } }", schema)).toEqual([]);
    // Other mistakes in the same query are still reported.
    const problems = diagnose("query { user(id: {{userId}}) { nope } }", schema);
    expect(problems).toHaveLength(1);
    expect(problems[0]?.message).toContain("nope");
  });
});

describe("the docs", () => {
  it("names the root types", () => {
    expect(rootTypes(schema)).toEqual([
      { label: "query", name: "Query" },
      { label: "mutation", name: "Mutation" },
    ]);
  });

  it("describes an object type", () => {
    const user = describeType(schema, "User")!;
    expect(user).toMatchObject({
      kind: "object",
      description: "Who can sign in.",
      implements: ["Node"],
    });
    const orders = user.fields.find((f) => f.name === "orders")!;
    expect(orders.type).toBe("[Order!]!");
    expect(orders.description).toBe("Orders, newest first.");
    expect(orders.args).toEqual([
      { name: "first", type: "Int", description: "", defaultValue: "10" },
      { name: "status", type: "Status", description: "" },
    ]);
    expect(user.fields.find((f) => f.name === "old")?.deprecated).toBe("Use name.");
  });

  it("describes unions, interfaces, enums and inputs", () => {
    expect(describeType(schema, "Result")).toMatchObject({
      kind: "union",
      possible: ["User", "Order"],
    });
    expect(describeType(schema, "Node")).toMatchObject({
      kind: "interface",
      possible: ["User", "Order"],
    });
    expect(describeType(schema, "Status")?.values.map((v) => v.name)).toEqual(["OPEN", "SHIPPED"]);
    expect(describeType(schema, "Filter")?.inputFields.map((f) => f.name)).toEqual(["text"]);
    expect(describeType(schema, "Missing")).toBeNull();
  });

  it("searches types, then fields", () => {
    const names = (text: string) => searchDocs(schema, text).map((m) => m.label);
    expect(names("order")).toEqual(["Order", "User.orders"]);
    expect(names("FILTER")).toEqual(["Filter"]);
    expect(names("zzz")).toEqual([]);
    expect(searchDocs(schema, "", 3)).toHaveLength(3);
    expect(searchDocs(schema, "").every((m) => !m.label.startsWith("__"))).toBe(true);
  });

  it("strips list and non-null marks", () => {
    expect(namedTypeOf("[Order!]!")).toBe("Order");
    expect(namedTypeOf("ID")).toBe("ID");
  });

  it("counts the types a schema defines", () => {
    // User, Node, Order, Result, Status, Filter, Query and Mutation.
    expect(typeCount(schema)).toBe(8);
  });
});
