// GraphQL support for the body editor: a schema read by introspection
// feeds completion, linting and the docs browser. This file and what it
// imports load only when a GraphQL body is first shown.

import { linter, type Diagnostic } from "@codemirror/lint";
import type { Extension } from "@codemirror/state";
import { completion, graphqlLanguageSupport, jump, stateExtensions } from "cm6-graphql";
import {
  buildClientSchema,
  isEnumType,
  isInputObjectType,
  isInterfaceType,
  isObjectType,
  isScalarType,
  isUnionType,
  parse,
  validateSchema,
  type GraphQLArgument,
  type GraphQLField,
  type GraphQLInputField,
  type GraphQLNamedType,
  type GraphQLSchema,
  type IntrospectionQuery,
} from "graphql";
import { getDiagnostics } from "graphql-language-service";

/** Builds a schema from the `data` of an introspection answer. */
export function schemaFrom(data: unknown): GraphQLSchema {
  return buildClientSchema(data as IntrospectionQuery);
}

const VARIABLE = /\{\{[^{}]*\}\}/g;

/**
 * Stands names in for `{{variables}}`, one underscore for each character,
 * so the text parses and the offsets stay where they were. Returns the
 * ranges that were replaced too.
 */
export function maskVariables(text: string): { masked: string; ranges: [number, number][] } {
  const ranges: [number, number][] = [];
  const masked = text.replace(VARIABLE, (match, offset: number) => {
    ranges.push([offset, offset + match.length]);
    return "_".repeat(match.length);
  });
  return { masked, ranges };
}

/** The names of the operations a query defines, in order. */
export function operationNames(query: string): string[] {
  const { masked } = maskVariables(query);
  try {
    return parse(masked).definitions.flatMap((definition) =>
      definition.kind === "OperationDefinition" && definition.name ? [definition.name.value] : [],
    );
  } catch {
    // Half-typed queries do not parse; read the names off the text.
    return [...masked.matchAll(/\b(?:query|mutation|subscription)\s+([_A-Za-z]\w*)/g)].flatMap(
      (match) => (match[1] ? [match[1]] : []),
    );
  }
}

export interface Problem {
  from: number;
  to: number;
  severity: "error" | "warning" | "info";
  message: string;
}

/**
 * What is wrong with a query: syntax errors always, and with a schema,
 * fields, arguments and types it does not have. Problems that touch a
 * `{{variable}}` are left out, since the variable is not known until the
 * request is sent.
 */
export function diagnose(text: string, schema: GraphQLSchema | null): Problem[] {
  if (!text.trim()) return [];
  if (schema && validateSchema(schema).length > 0) schema = null;
  const { masked, ranges } = maskVariables(text);
  const starts = [0];
  for (let i = 0; i < text.length; i++) if (text[i] === "\n") starts.push(i + 1);
  const offset = (line: number, character: number): number =>
    Math.min(text.length, (starts[Math.min(line, starts.length - 1)] ?? 0) + character);
  const problems: Problem[] = [];
  for (const item of getDiagnostics(masked, schema)) {
    const from = offset(item.range.start.line, item.range.start.character);
    let to = offset(item.range.end.line, item.range.end.character);
    if (to <= from) to = Math.min(text.length, from + 1);
    if (ranges.some(([start, end]) => from < end && to > start)) continue;
    problems.push({
      from,
      to,
      severity: item.severity === 2 ? "warning" : item.severity === 1 ? "error" : "info",
      message: typeof item.message === "string" ? item.message : item.message.value,
    });
  }
  return problems;
}

export interface GraphqlOptions {
  /** Told where Ctrl-click on a field or type should open the docs. */
  onShowInDocs?: (field?: string, type?: string, parentType?: string) => void;
}

/** The editor extensions for GraphQL, with or without a schema. */
export function graphqlExtensions(
  schema: GraphQLSchema | null,
  options: GraphqlOptions = {},
): Extension {
  return [
    graphqlLanguageSupport(),
    completion,
    jump,
    stateExtensions(schema ?? undefined, { onShowInDocs: options.onShowInDocs }),
    linter(
      (view) =>
        diagnose(view.state.doc.toString(), schema).map((problem): Diagnostic => ({
          from: problem.from,
          to: problem.to,
          severity: problem.severity,
          message: problem.message,
        })),
      { delay: 400 },
    ),
  ];
}

// ---- The docs browser ------------------------------------------------------

export type TypeKind = "object" | "interface" | "union" | "enum" | "input" | "scalar";

export interface DocsEntry {
  name: string;
  type: string;
  description: string;
  args: { name: string; type: string; description: string; defaultValue?: string }[];
  deprecated?: string;
}

/** Everything the docs browser shows for one type. */
export interface DocsType {
  name: string;
  kind: TypeKind;
  description: string;
  /** Interfaces an object implements. */
  implements: string[];
  fields: DocsEntry[];
  inputFields: DocsEntry[];
  values: { name: string; description: string; deprecated?: string }[];
  /** The members of a union, or the types that implement an interface. */
  possible: string[];
}

function kindOf(type: GraphQLNamedType): TypeKind {
  if (isObjectType(type)) return "object";
  if (isInterfaceType(type)) return "interface";
  if (isUnionType(type)) return "union";
  if (isEnumType(type)) return "enum";
  if (isInputObjectType(type)) return "input";
  return "scalar";
}

function argument(arg: GraphQLArgument): DocsEntry["args"][number] {
  return {
    name: arg.name,
    type: String(arg.type),
    description: arg.description ?? "",
    ...(arg.defaultValue !== undefined ? { defaultValue: JSON.stringify(arg.defaultValue) } : {}),
  };
}

function entry(field: GraphQLField<unknown, unknown> | GraphQLInputField): DocsEntry {
  const args = "args" in field ? field.args.map(argument) : [];
  const reason = "deprecationReason" in field ? field.deprecationReason : undefined;
  return {
    name: field.name,
    type: String(field.type),
    description: field.description ?? "",
    args,
    ...(reason ? { deprecated: reason } : {}),
  };
}

/** The type with this name, as the docs browser shows it. */
export function describeType(schema: GraphQLSchema, name: string): DocsType | null {
  const type = schema.getType(name);
  if (!type) return null;
  const docs: DocsType = {
    name: type.name,
    kind: kindOf(type),
    description: type.description ?? "",
    implements: [],
    fields: [],
    inputFields: [],
    values: [],
    possible: [],
  };
  if (isObjectType(type) || isInterfaceType(type)) {
    docs.fields = Object.values(type.getFields()).map(entry);
    docs.implements = type.getInterfaces().map((i) => i.name);
  }
  if (isInterfaceType(type) || isUnionType(type)) {
    docs.possible = schema.getPossibleTypes(type).map((t) => t.name);
  }
  if (isInputObjectType(type)) docs.inputFields = Object.values(type.getFields()).map(entry);
  if (isEnumType(type)) {
    docs.values = type.getValues().map((v) => ({
      name: v.name,
      description: v.description ?? "",
      ...(v.deprecationReason ? { deprecated: v.deprecationReason } : {}),
    }));
  }
  return docs;
}

/** The query, mutation and subscription types, with what each is for. */
export function rootTypes(schema: GraphQLSchema): { label: string; name: string }[] {
  const roots = [
    ["query", schema.getQueryType()],
    ["mutation", schema.getMutationType()],
    ["subscription", schema.getSubscriptionType()],
  ] as const;
  return roots.flatMap(([label, type]) => (type ? [{ label, name: type.name }] : []));
}

export interface DocsMatch {
  /** A type's name, or `Type.field` for a field. */
  label: string;
  /** The type to open. */
  type: string;
  kind: TypeKind | "field";
}

/**
 * Types and fields whose names contain the text, ignoring case, types
 * first and each group in alphabetical order. Built-in `__` types stay
 * out. At most `limit` matches are returned.
 */
export function searchDocs(schema: GraphQLSchema, text: string, limit = 100): DocsMatch[] {
  const needle = text.trim().toLowerCase();
  const types = Object.values(schema.getTypeMap())
    .filter((t) => !t.name.startsWith("__"))
    .sort((a, b) => a.name.localeCompare(b.name));
  const out: DocsMatch[] = [];
  for (const type of types) {
    if (!needle || type.name.toLowerCase().includes(needle)) {
      out.push({ label: type.name, type: type.name, kind: kindOf(type) });
      if (out.length >= limit) return out;
    }
  }
  if (!needle) return out;
  for (const type of types) {
    if (!isObjectType(type) && !isInterfaceType(type) && !isInputObjectType(type)) continue;
    for (const field of Object.values(type.getFields())) {
      if (field.name.toLowerCase().includes(needle)) {
        out.push({ label: `${type.name}.${field.name}`, type: type.name, kind: "field" });
        if (out.length >= limit) return out;
      }
    }
  }
  return out;
}

/** The name of the type a field or argument ends in, without `!` or `[]`. */
export function namedTypeOf(type: string): string {
  return type.replace(/[[\]!]/g, "");
}

/** A count for the toolbar: the types the schema defines, without built-ins. */
export function typeCount(schema: GraphQLSchema): number {
  return Object.values(schema.getTypeMap()).filter(
    (t) => !t.name.startsWith("__") && !(isScalarType(t) && isBuiltinScalar(t.name)),
  ).length;
}

function isBuiltinScalar(name: string): boolean {
  return ["String", "Int", "Float", "Boolean", "ID"].includes(name);
}
