import { describe, expect, it } from "vitest";
import { fingerprint, newDraft, toDraft, toRequest } from "./draft";
import {
  appendCapped,
  closeText,
  fromPicker,
  matches,
  outboundFrom,
  newComposer,
  pickerValue,
  preview,
  readable,
  splitList,
  type LogEntry,
  type StreamEvent,
} from "./realtime";

const message = (data: string, extra: Partial<StreamEvent> = {}): LogEntry => ({
  seq: 0,
  event: {
    at: 0,
    type: "message",
    direction: "received",
    data,
    size: data.length,
    ...extra,
  } as StreamEvent,
});

describe("drafts", () => {
  it("leave HTTP requests as they were", () => {
    const request = toRequest(newDraft("Plain"));
    expect(request).not.toHaveProperty("kind");
    expect(request).not.toHaveProperty("websocket");
  });

  it("keep only the section of their kind, without empty values", () => {
    const draft = toDraft({
      name: "Chat",
      kind: "websocket",
      method: "GET",
      url: "wss://x.test",
      websocket: { subprotocols: ["chat.v2"], messages: [{ name: "Hi", content: "hi" }] },
      socketio: { namespace: "/left-over" },
    });
    expect(draft.websocket.messages[0]).toEqual({ name: "Hi", type: "text", content: "hi" });
    const request = toRequest(draft);
    expect(request.kind).toBe("websocket");
    expect(request.websocket).toEqual({
      subprotocols: ["chat.v2"],
      messages: [{ name: "Hi", content: "hi" }],
    });
    expect(request).not.toHaveProperty("socketio");

    draft.kind = "socketio";
    draft.socketio.events = [" a ", ""];
    draft.socketio.messages = [{ name: "", event: "join", args: "", ack: true }];
    expect(toRequest(draft).socketio).toEqual({
      namespace: "/left-over",
      events: ["a"],
      messages: [{ event: "join", ack: true }],
    });

    draft.kind = "sse";
    expect(toRequest(draft)).not.toHaveProperty("sse");
    draft.sse.reconnect = true;
    expect(toRequest(draft).sse).toEqual({ reconnect: true });
  });

  it("keep the Socket.IO transport and the until limits only when they are set", () => {
    const draft = toDraft({
      name: "Orders",
      kind: "socketio",
      method: "GET",
      url: "https://x.test",
    });
    expect(draft.socketio.transport).toBe("auto");
    expect(draft.until).toEqual({ messages: null, time_ms: null });
    const plain = toRequest(draft);
    expect(plain).not.toHaveProperty("socketio");
    expect(plain).not.toHaveProperty("until");

    draft.socketio.transport = "websocket";
    draft.until.messages = 3;
    draft.until.time_ms = 2500.4;
    const request = toRequest(draft);
    expect(request.socketio).toEqual({ transport: "websocket" });
    expect(request.until).toEqual({ messages: 3, time_ms: 2500 });
    expect(toDraft(request).until).toEqual({ messages: 3, time_ms: 2500 });
    expect(toDraft(request).socketio.transport).toBe("websocket");

    // Zero messages is a limit; zero milliseconds is none, nor is a fraction.
    draft.until.messages = 0;
    draft.until.time_ms = 0;
    expect(toRequest(draft).until).toEqual({ messages: 0 });
    draft.until.messages = 1.5;
    expect(toRequest(draft)).not.toHaveProperty("until");

    // Any realtime kind has them, and HTTP requests do not.
    draft.kind = "sse";
    draft.until.messages = 4;
    expect(toRequest(draft).until).toEqual({ messages: 4 });
    expect(toRequest(draft)).not.toHaveProperty("socketio");
  });

  it("are dirty when the kind changes", () => {
    const draft = newDraft();
    const before = fingerprint(draft);
    draft.kind = "sse";
    expect(fingerprint(draft)).not.toBe(before);
  });
});

describe("the picker", () => {
  it("round-trips methods and kinds", () => {
    expect(pickerValue("http", "POST")).toBe("POST");
    expect(pickerValue("sse", "POST")).toBe("sse:POST");
    expect(pickerValue("websocket", "GET")).toBe("websocket");
    expect(fromPicker("sse:GET")).toEqual({ kind: "sse", method: "GET" });
    expect(fromPicker("socketio")).toEqual({ kind: "socketio", method: null });
    expect(fromPicker("DELETE")).toEqual({ kind: "http", method: "DELETE" });
  });
});

describe("the log", () => {
  it("drops the oldest entries past its limits", () => {
    const log: LogEntry[] = [];
    let chars = 0;
    for (let i = 0; i < 5; i++) {
      const added = { ...message("x".repeat(10)), seq: i };
      ({ chars } = appendCapped(log, [added], chars, { entries: 3, chars: 1000 }));
    }
    expect(log.map((e) => e.seq)).toEqual([2, 3, 4]);
    const result = appendCapped(log, [{ ...message("y".repeat(25)), seq: 5 }], chars, {
      entries: 10,
      chars: 40,
    });
    expect(result.dropped).toBe(2);
    expect(log.map((e) => e.seq)).toEqual([4, 5]);
    expect(result.chars).toBe(35);
  });

  it("keeps the newest entry even when it is too big alone", () => {
    const log: LogEntry[] = [];
    appendCapped(log, [message("z".repeat(100))], 0, { entries: 10, chars: 10 });
    expect(log).toHaveLength(1);
  });

  it("filters by text and direction", () => {
    const hello = message('{"type":"hello"}', { event: "greeting" } as Partial<StreamEvent>);
    expect(matches(hello, "HELLO", "all")).toBe(true);
    expect(matches(hello, "greeting", "received")).toBe(true);
    expect(matches(hello, "", "sent")).toBe(false);
    const note: LogEntry = { seq: 1, event: { at: 0, type: "note", message: "Reconnecting" } };
    expect(matches(note, "reconn", "all")).toBe(true);
    expect(matches(note, "", "received")).toBe(false);
  });
});

describe("messages", () => {
  it("read as indented JSON when they are JSON", () => {
    const json = message('{"a":[1,2]}').event;
    if (json.type !== "message") throw new Error();
    expect(readable(json)).toEqual({ text: '{\n  "a": [\n    1,\n    2\n  ]\n}', json: true });
    const text = message("{not json").event;
    if (text.type !== "message") throw new Error();
    expect(readable(text).json).toBe(false);
    expect(preview({ ...text, data: "a\n  b" })).toBe("a b");
  });

  it("are built from the composer", () => {
    const composer = { ...newComposer(), content: "hi" };
    expect(outboundFrom("websocket", composer)).toEqual({
      message: { type: "text", content: "hi" },
    });
    expect(outboundFrom("socketio", composer)).toBe("Name the event to emit.");
    expect(outboundFrom("socketio", { ...composer, event: " join ", args: "[1]" })).toEqual({
      emit: { event: "join", args: "[1]", ack: false },
    });
    expect(typeof outboundFrom("sse", composer)).toBe("string");
  });

  it("explain closes", () => {
    expect(closeText(1000, "", true)).toBe("Disconnected (1000 normal)");
    expect(closeText(4001, "bye", false)).toBe("The server closed the connection (4001): bye");
    expect(closeText(null, "", false)).toBe("The server closed the connection");
  });

  it("split typed lists", () => {
    expect(splitList("a, b\nc,,")).toEqual(["a", "b", "c"]);
  });
});
