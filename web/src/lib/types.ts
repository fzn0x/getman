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
  | { type: "graphql"; query: string; variables?: string; operation_name?: string };

export type BodyType = Body["type"];

export type OAuth2Grant = "client_credentials" | "password" | "authorization_code" | "implicit";

export interface OAuth2 {
  type: "oauth2";
  grant: OAuth2Grant;
  auth_url?: string;
  token_url: string;
  client_id: string;
  client_secret?: string;
  callback_url?: string;
  pkce?: "off" | "s256" | "plain";
  scope?: string;
  audience?: string;
  resource?: string;
  username?: string;
  password?: string;
  client_auth: "basic" | "body";
  auth_params?: KeyValue[];
  token_params?: KeyValue[];
  prefix?: string;
  token_in?: "header" | "query";
}

export interface AwsSigV4 {
  type: "aws_sigv4";
  access_key: string;
  secret_key: string;
  session_token?: string;
  region: string;
  service: string;
  in?: "header" | "query";
  expires_in?: number | null;
  unsigned_payload?: boolean;
}

export type JwtAlgorithm =
  | "HS256"
  | "HS384"
  | "HS512"
  | "RS256"
  | "RS384"
  | "RS512"
  | "PS256"
  | "PS384"
  | "PS512"
  | "ES256"
  | "ES384";

export interface Jwt {
  type: "jwt";
  algorithm?: JwtAlgorithm;
  secret?: string;
  secret_base64?: boolean;
  private_key?: string;
  private_key_file?: string;
  header?: string;
  payload?: string;
  issued_at?: boolean;
  expires_in?: number | null;
  in?: "header" | "query";
  prefix?: string;
  param?: string;
}

export interface Asap {
  type: "asap";
  algorithm?: JwtAlgorithm | null;
  issuer: string;
  subject?: string;
  audience: string;
  key_id: string;
  private_key?: string;
  private_key_file?: string;
  claims?: string;
  expires_in?: number | null;
}

export type OAuth1Method =
  | "HMAC-SHA1"
  | "HMAC-SHA256"
  | "HMAC-SHA512"
  | "PLAINTEXT"
  | "RSA-SHA1"
  | "RSA-SHA256"
  | "RSA-SHA512";

export interface OAuth1 {
  type: "oauth1";
  signature_method?: OAuth1Method;
  consumer_key: string;
  consumer_secret?: string;
  token?: string;
  token_secret?: string;
  private_key?: string;
  private_key_file?: string;
  realm?: string;
  callback?: string;
  verifier?: string;
  version?: string;
  timestamp?: string;
  nonce?: string;
  body_hash?: boolean;
  in?: "header" | "query" | "body";
}

export interface Hawk {
  type: "hawk";
  id: string;
  key: string;
  algorithm?: "sha256" | "sha1";
  ext?: string;
  app?: string;
  dlg?: string;
  payload_hash?: boolean;
}

export interface EdgeGrid {
  type: "edgegrid";
  client_token: string;
  client_secret: string;
  access_token: string;
  base_url?: string;
  headers_to_sign?: string[];
  max_body?: number | null;
}

export type Auth =
  | { type: "inherit" }
  | { type: "none" }
  | { type: "basic"; username: string; password: string }
  | { type: "bearer"; token: string; prefix?: string }
  | { type: "api_key"; key: string; value: string; in: "header" | "query" }
  | { type: "digest"; username: string; password: string }
  | OAuth2
  | AwsSigV4
  | Jwt
  | OAuth1
  | Hawk
  | EdgeGrid
  | Asap;

export type AuthType = Auth["type"];

export interface Scripts {
  pre_request?: string;
  post_response?: string;
}

export interface Settings {
  timeout_ms?: number | null;
  follow_redirects?: boolean | null;
  max_redirects?: number | null;
  follow_original_method?: boolean | null;
  follow_authorization?: boolean | null;
  verify_tls?: boolean | null;
  http_version?: HttpVersion | null;
  title_case_headers?: boolean | null;
  /** Written as just the URL (`"off"`, `"system"`, a proxy URL) when that is all it has. */
  proxy?: ProxySetting | string | null;
  /** An empty one stops a folder's certificate from being used. */
  client_cert?: ClientCert | null;
  ca_certs?: string[] | null;
  cookies?: boolean | null;
  max_response_mb?: number | null;
}

export type HttpVersion = "auto" | "http1" | "http2" | "http3";

export interface ProxySetting {
  url: string;
  username?: string;
  password?: string;
  bypass?: string;
}

export interface ClientCert {
  cert?: string;
  key?: string;
  pkcs12?: string;
  passphrase?: string;
}

/** What a request file holds; `http` when the file says nothing. */
export type RequestKind = "http" | "websocket" | "sse" | "socketio" | "grpc";

export type MessageFormat = "text" | "json" | "binary";

/** A WebSocket message saved with a request. */
export interface SavedMessage {
  name?: string;
  type?: MessageFormat;
  content: string;
}

/** A Socket.IO event saved with a request. */
export interface SavedEmit {
  name?: string;
  event: string;
  args?: string;
  ack?: boolean;
}

export interface WebSocketSettings {
  subprotocols?: string[];
  messages?: SavedMessage[];
}

export interface SseSettings {
  reconnect?: boolean;
}

/** How a Socket.IO connection reaches its server; `auto` when missing. */
export type SocketIoTransport = "auto" | "websocket" | "polling";

/** When `getman run` ends a connection: the first limit reached. */
export interface Until {
  messages?: number;
  time_ms?: number;
}

export interface SocketIoSettings {
  namespace?: string;
  path?: string;
  transport?: SocketIoTransport;
  auth?: string;
  events?: string[];
  messages?: SavedEmit[];
}

export interface Request {
  name: string;
  /** What the request speaks; HTTP when missing. */
  kind?: RequestKind;
  method: string;
  url: string;
  grpc?: Grpc;
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
  /** Saved responses. The editors leave them alone; they change through
   * their own API calls. */
  examples?: Example[];
  websocket?: WebSocketSettings;
  sse?: SseSettings;
  socketio?: SocketIoSettings;
  until?: Until;
}

/** A request with every optional part present, as the editors use it. */
export interface Draft {
  name: string;
  kind: RequestKind;
  method: string;
  url: string;
  grpc: { proto: string; tls: boolean; message: string };
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
  websocket: { subprotocols: string[]; messages: Required<SavedMessage>[] };
  sse: { reconnect: boolean };
  socketio: {
    namespace: string;
    path: string;
    transport: SocketIoTransport;
    auth: string;
    events: string[];
    messages: Required<SavedEmit>[];
  };
  /** Empty fields are not set. */
  until: { messages: number | null; time_ms: number | null };
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
  | {
      kind: "request";
      path: string;
      name: string;
      method: string;
      request_kind?: RequestKind;
      error?: string;
    };

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
  /** Scripts are turned off for this workspace, on this machine. */
  scripts_off?: boolean;
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
  /** The redirects followed on the way, in order. */
  hops?: Hop[];
  remote_addr?: string;
  timings?: Timings;
  tls?: TlsDetails;
  /** The server offered HTTP/3 in Alt-Svc while the request used auto. */
  http3_available?: boolean;
  /** The body is an event stream that was still open when the request timed out. */
  stream_open?: boolean;
}

/** A redirect followed on the way to a response. */
export interface Hop {
  status: number;
  /** The URL that was requested and answered with the redirect. */
  url: string;
  time_ms: number;
}

/** Where a response's time went, in milliseconds. */
export interface Timings {
  dns_ms: number;
  /** `null` over HTTP/3, where the QUIC handshake is part of the first byte. */
  connect_ms: number | null;
  first_byte_ms: number;
  download_ms: number;
  reused: boolean;
}

export interface TlsDetails {
  version?: string;
  certificate?: CertificateInfo;
}

export interface CertificateInfo {
  subject: string;
  issuer: string;
  names?: string[];
  not_before: number;
  not_after: number;
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
  scope: "environment" | "collection" | "global" | "session" | "local";
  name: string;
  value: string | null;
}

export interface Execution {
  path: string | null;
  name: string;
  request: Outgoing | null;
  /** The request as sent with the environment's secret values masked, when it held any. */
  request_masked?: Outgoing;
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
  cancelled?: boolean;
  grpc?: GrpcCall;
}

export interface ResolvedVar {
  name: string;
  value: string;
  scope:
    "global" | "workspace" | "environment" | "folder" | "request" | "data" | "session" | "local";
  /** The environment marks it secret; mask the value until asked. */
  secret?: boolean;
}

export interface DynamicVar {
  name: string;
  description: string;
}

/** A GraphQL server's schema: the `data` of its introspection answer. */
export interface GraphqlSchemaAnswer {
  /** `null` when nothing is kept for the URL and none was asked for. */
  schema: unknown;
  /** The URL the schema belongs to, with variables filled in. */
  url: string;
  /** Whether it came from memory, with nothing sent. */
  cached?: boolean;
  /** When it was read, in seconds since 1970. */
  fetched_at?: number;
  status?: number;
  time_ms?: number;
}

export interface Language {
  id: string;
  label: string;
  syntax: string;
  /** Whether the snippet has an indent the options change. */
  indent: boolean;
  /** Whether the snippet has imports or a main function to leave out. */
  boilerplate: boolean;
}

export interface Cookie {
  domain: string;
  path: string;
  name: string;
  value: string;
  expires: number | null;
  secure: boolean;
  http_only: boolean;
  /** Sent to the domain only, not its subdomains. */
  host_only: boolean;
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
  /** Size of the response body, in bytes. */
  size?: number | null;
  error: string | null;
  request: Request;
}

/** A history entry with the response kept with it. */
export interface HistoryDetail extends HistoryEntry {
  response: Response | null;
  /** History kept only the first MiB of the body. */
  body_cut: boolean;
}

export interface TrashItem {
  id: string;
  path: string;
  kind: "request" | "folder";
  name: string;
  method?: string;
  deleted_at: number;
}

export interface HeaderPreset {
  name: string;
  headers: KeyValue[];
}

export interface ImportReport {
  kind: "collection" | "environment" | "globals" | "request";
  name: string;
  path: string;
  requests: number;
  folders: number;
  examples?: number;
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
  /** Milliseconds since the Unix epoch. */
  expires_at?: number;
  refreshable?: boolean;
}

/** A browser sign-in Getman started. */
export interface Authorization {
  id: string;
  url: string;
  redirect_uri: string;
  notes: string[];
  /** Whether Getman could open the system's browser. */
  opened: boolean;
}

// ---- Examples, past runs and mock servers ----------------------------------

/** A saved response, kept with its request. */
export interface Example {
  name: string;
  status: number;
  headers?: KeyValue[];
  /** The body, when it is short text. */
  body?: string;
  /** A file in the request's `<name>.examples` directory holding the body. */
  body_file?: string | null;
  request?: ExampleRequest | null;
}

export interface ExampleRequest {
  method?: string;
  url?: string;
  headers?: KeyValue[];
  body?: string;
}

/** What a run covered, where and when. */
export interface RunAbout {
  workspace: string;
  targets: string[];
  environment: string | null;
  /** Milliseconds since the Unix epoch. */
  started: number;
}

/** A kept run, as listed. */
export interface RunListing {
  id: string;
  about: RunAbout;
  iterations: number;
  requests: number;
  summary: RunSummary | null;
}

/** A kept run with its results. Bodies and headers are not kept. */
export interface StoredRun {
  id: string;
  about: RunAbout;
  iterations: number;
  items: RunItem[];
  results: { iteration: number; index: number; execution: Execution }[];
  summary: RunSummary | null;
}

export type ReportFormat = "html" | "json" | "junit" | "csv" | "xlsx";

export interface MockedRoute {
  method: string;
  path: string;
  request: string;
  examples: { name: string; status: number }[];
}

export interface MockHit {
  at: number;
  method: string;
  path: string;
  status: number;
  request: string | null;
  example: string | null;
}

/** A mock server started from the UI. */
export interface MockServer {
  folder: string;
  url: string;
  port: number;
  delay_ms: number;
  mocked: MockedRoute[];
  hits: MockHit[];
}

// ---- gRPC ---------------------------------------------------------------

export interface Grpc {
  /** A .proto file or folder; server reflection when empty. */
  proto?: string;
  tls?: boolean;
  /** JSON; streams from the client hold several messages in a row. */
  message?: string;
}

export type CallType = "unary" | "server_streaming" | "client_streaming" | "bidi_streaming";

export interface StreamMessage {
  direction: "sent" | "received";
  message: unknown;
  time_ms: number;
  size: number;
  error?: string;
}

export interface GrpcStatus {
  code: number;
  name: string;
  message: string;
  details?: unknown;
}

export interface GrpcCall {
  method: string;
  call_type: CallType;
  address: string;
  tls: boolean;
  status: GrpcStatus | null;
  headers: Header[];
  trailers: Header[];
  messages: StreamMessage[];
  dropped_messages?: number;
  time_ms: number;
  headers_ms: number | null;
}

export interface GrpcMethod {
  name: string;
  /** What a request's `method` holds: `package.Service/Method`. */
  path: string;
  input: string;
  output: string;
  call_type: CallType;
  example: unknown;
}

export interface GrpcService {
  name: string;
  methods: GrpcMethod[];
}

export type GrpcEvent =
  | { event: "started"; method: string; call_type: CallType; address: string }
  | { event: "headers"; headers: Header[]; time_ms: number }
  | { event: "message"; message: StreamMessage }
  | { event: "done"; execution: Execution };
