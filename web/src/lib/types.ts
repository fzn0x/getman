// The shapes the local API speaks, mirroring the Rust types in
// crates/getman-core.

export type Vars = Record<string, string>;

export interface KeyValue {
  name: string;
  value: string;
  enabled?: boolean;
  description?: string;
}

export interface FormField {
  name: string;
  value?: string;
  file?: string | null;
  content_type?: string | null;
  enabled?: boolean;
}

export type Body =
  | { type: "none" }
  | { type: "json"; content: string }
  | { type: "xml"; content: string }
  | { type: "text"; content: string; content_type?: string | null }
  | { type: "form"; fields: KeyValue[] }
  | { type: "multipart"; fields: FormField[] }
  | { type: "binary"; file: string; content_type?: string | null }
  | { type: "graphql"; query: string; variables?: string };

export type BodyType = Body["type"];

export interface OAuth2 {
  type: "oauth2";
  grant: "client_credentials" | "password";
  token_url: string;
  client_id: string;
  client_secret?: string;
  scope?: string;
  audience?: string;
  username?: string;
  password?: string;
  client_auth: "basic" | "body";
}

export type Auth =
  | { type: "inherit" }
  | { type: "none" }
  | { type: "basic"; username: string; password: string }
  | { type: "bearer"; token: string; prefix?: string }
  | { type: "api_key"; key: string; value: string; in: "header" | "query" }
  | { type: "digest"; username: string; password: string }
  | OAuth2;

export type AuthType = Auth["type"];

export interface Scripts {
  pre_request?: string;
  post_response?: string;
}

export interface Settings {
  timeout_ms?: number | null;
  follow_redirects?: boolean | null;
  max_redirects?: number | null;
  verify_tls?: boolean | null;
}

export interface Request {
  name: string;
  method: string;
  url: string;
  description?: string;
  path_params?: Vars;
  disabled_params?: KeyValue[];
  headers?: KeyValue[];
  body?: Body;
  auth?: Auth;
  variables?: Vars;
  scripts?: Scripts;
  checks?: string[];
  captures?: Vars;
  settings?: Settings;
}

/** A request with every optional part present, as the editors use it. */
export interface Draft {
  name: string;
  method: string;
  url: string;
  description: string;
  path_params: Vars;
  disabled_params: KeyValue[];
  headers: KeyValue[];
  body: Body;
  auth: Auth;
  variables: Vars;
  scripts: { pre_request: string; post_response: string };
  checks: string[];
  captures: Vars;
  settings: Settings;
}

export interface Folder {
  name: string;
  description?: string;
  variables?: Vars;
  headers?: KeyValue[];
  auth?: Auth;
  scripts?: Scripts;
  settings?: Settings;
  order?: string[];
}

export type Node =
  | { kind: "folder"; path: string; name: string; children: Node[] }
  | { kind: "request"; path: string; name: string; method: string; error?: string };

export interface EnvironmentSummary {
  id: string;
  name: string;
}

export interface Environment {
  name?: string;
  variables?: Vars;
  secrets?: string[];
}

export interface EnvironmentFiles {
  shared: Environment;
  local: Vars;
}

export interface LocalState {
  variables?: Vars;
  globals?: Vars;
}

export interface WorkspaceInfo {
  name: string;
  root: string;
  version: string;
  tree: Node[];
  environments: EnvironmentSummary[];
}

export type Payload = { text: string } | { base64: string };

export interface Header {
  name: string;
  value: string;
}

export type OutgoingBody =
  | { type: "none" }
  | { type: "bytes"; data: Payload }
  | { type: "file"; path: string }
  | {
      type: "multipart";
      parts: { name: string; value?: string; file?: string; content_type?: string }[];
    };

export interface Outgoing {
  method: string;
  url: string;
  headers: Header[];
  body: OutgoingBody;
}

export interface Response {
  status: number;
  status_text: string;
  http_version: string;
  url: string;
  headers: Header[];
  body: Payload;
  truncated: boolean;
  time_ms: number;
  headers_ms: number;
  size: { headers: number; body: number };
  remote_addr?: string;
}

export interface TestResult {
  name: string;
  passed: boolean;
  message?: string;
  skipped?: boolean;
}

export interface LogEntry {
  level: "info" | "warn" | "error" | "debug";
  message: string;
  origin: string;
}

export interface VarChange {
  scope: "environment" | "collection" | "global" | "local";
  name: string;
  value: string | null;
}

export interface Execution {
  path: string | null;
  name: string;
  request: Outgoing | null;
  response: Response | null;
  error: string | null;
  tests: TestResult[];
  checks: TestResult[];
  logs: LogEntry[];
  script_errors: { origin: string; message: string }[];
  changes: VarChange[];
  missing_variables: string[];
  warnings: string[];
  skipped: boolean;
  next_request: unknown;
}

export interface ResolvedVar {
  name: string;
  value: string;
  scope: "global" | "workspace" | "environment" | "folder" | "request" | "data" | "local";
}

export interface DynamicVar {
  name: string;
  description: string;
}

export interface Language {
  id: string;
  label: string;
  syntax: string;
}

export interface Cookie {
  domain: string;
  path: string;
  name: string;
  value: string;
  expires: number | null;
  secure: boolean;
  http_only: boolean;
}

export interface HistoryEntry {
  id: string;
  at: number;
  path: string | null;
  environment: string | null;
  name: string;
  method: string;
  url: string;
  status: number | null;
  time_ms: number | null;
  error: string | null;
  request: Request;
}

export interface ImportReport {
  kind: "collection" | "environment" | "globals" | "request";
  name: string;
  path: string;
  requests: number;
  folders: number;
  warnings: string[];
  /** The format read, such as "OpenAPI 3.1.0". */
  format?: string;
  /** Ids of environments created alongside a collection. */
  environments?: string[];
}

/** A file written by an export, to download. */
export interface Exported {
  file_name: string;
  content: string;
  warnings: string[];
}

export interface RunItem {
  path: string;
  name: string;
  method: string;
}

export interface RunSummary {
  iterations: number;
  requests: number;
  failed_requests: number;
  skipped_requests: number;
  tests_passed: number;
  tests_failed: number;
  checks_passed: number;
  checks_failed: number;
  duration_ms: number;
  average_response_ms: number;
  stopped: string | null;
  warnings: string[];
}

export type RunEvent =
  | { event: "start"; requests: RunItem[]; iterations: number }
  | { event: "iteration"; iteration: number }
  | { event: "request"; iteration: number; index: number; execution: Execution }
  | { event: "finish"; summary: RunSummary }
  | { event: "error"; error: string };

export interface Token {
  access_token: string;
  token_type: string;
  expires_in: number | null;
  scope: string | null;
}
