import { describe, expect, it } from "vitest";
import { newGrpcDraft, toDraft, toRequest } from "./draft";
import {
  clientStreams,
  countMessages,
  findMethod,
  formatMessages,
  requestMark,
  schemaSources,
  serverStreams,
} from "./grpc";
import type { GrpcService } from "./types";

describe("grpc", () => {
  it("counts messages written one after another", () => {
    expect(countMessages("")).toBe(0);
    expect(countMessages('{"a": 1}')).toBe(1);
    expect(countMessages('{"a": 1}\n{"a": 2} {}')).toBe(3);
    expect(countMessages('{"text": "{{who}} says }"} {"n": {{n}}}')).toBe(2);
    expect(countMessages('{"a": ')).toBeNull();
    expect(countMessages("{oops}")).toBeNull();
  });

  it("formats each message and keeps variables", () => {
    expect(formatMessages('{"a":1}{"b":"x {{who}}","n":{{n}}}')).toBe(
      '{\n  "a": 1\n}\n{\n  "b": "x {{who}}",\n  "n": {{n}}\n}\n',
    );
    expect(formatMessages("{nope")).toBeNull();
  });

  it("offers folders and files as schema sources", () => {
    expect(schemaSources(["protos/shop/orders.proto", "protos/common.proto"])).toEqual([
      { value: "protos", label: "protos/ (every .proto)" },
      { value: "protos/shop", label: "protos/shop/ (every .proto)" },
      { value: "protos/common.proto", label: "protos/common.proto" },
      { value: "protos/shop/orders.proto", label: "protos/shop/orders.proto" },
    ]);
  });

  it("keeps the kind, the method's case and the grpc block in drafts", () => {
    const request = toRequest(
      toDraft({
        name: "Watch",
        kind: "grpc",
        method: "shop.Orders/Watch",
        url: "localhost:50051",
        grpc: { proto: "protos", message: "{}" },
      }),
    );
    expect(request.kind).toBe("grpc");
    expect(request.method).toBe("shop.Orders/Watch");
    expect(request.grpc).toEqual({ proto: "protos", tls: false, message: "{}" });

    const http = toRequest(toDraft({ name: "Get", method: "get", url: "https://example.com" }));
    expect(http.method).toBe("GET");
    expect("kind" in http).toBe(false);
    expect("grpc" in http).toBe(false);
    expect(newGrpcDraft().kind).toBe("grpc");
  });

  it("finds methods and marks requests", () => {
    const services: GrpcService[] = [
      {
        name: "shop.Orders",
        methods: [
          {
            name: "Watch",
            path: "shop.Orders/Watch",
            input: "shop.Id",
            output: "shop.Order",
            call_type: "server_streaming",
            example: {},
          },
        ],
      },
    ];
    const method = findMethod(services, "/shop.Orders/Watch");
    expect(method?.name).toBe("Watch");
    expect(serverStreams(method?.call_type)).toBe(true);
    expect(clientStreams(method?.call_type)).toBe(false);
    expect(findMethod(services, "shop.Orders/Nope")).toBeUndefined();
    expect(requestMark({ kind: "grpc", method: "shop.Orders/Watch" })).toBe("GRPC");
    expect(requestMark({ method: "POST" })).toBe("POST");
  });
});
