// Getman's script runtime, evaluated before the scripts of a request.
//
// It defines `gm` (also reachable as `pm`), `console`, `require`, timers,
// `btoa`/`atob` and `crypto.randomUUID`. Rust hands in a JSON snapshot of
// the request, the response and the variables as `__gm_input`, and reads
// back what the scripts did from `__gm_finish()`.
"use strict";

(() => {
  const native = globalThis.__gm_native;
  const input = JSON.parse(globalThis.__gm_input);
  delete globalThis.__gm_native;
  delete globalThis.__gm_input;

  const out = {
    tests: [],
    logs: [],
    changes: [],
    request: null,
    next_request: undefined,
    skip_request: false,
  };
  let origin = "";
  const hasOwn = (object, key) => Object.prototype.hasOwnProperty.call(object, key);

  // ---- Formatting --------------------------------------------------------

  function inspect(value, depth = 0) {
    if (value === undefined) return "undefined";
    if (value === null) return "null";
    if (typeof value === "string") return JSON.stringify(value);
    if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint") {
      return String(value);
    }
    if (typeof value === "function") return `[Function${value.name ? " " + value.name : ""}]`;
    if (typeof value === "symbol") return value.toString();
    if (value instanceof Error) return `${value.name}: ${value.message}`;
    if (value instanceof RegExp) return String(value);
    if (value instanceof Date) return value.toISOString();
    if (depth > 2) return Array.isArray(value) ? "[…]" : "{…}";
    if (Array.isArray(value)) {
      const items = value.slice(0, 20).map((item) => inspect(item, depth + 1));
      if (value.length > 20) items.push(`… ${value.length - 20} more`);
      return `[ ${items.join(", ")} ]`.replace("[  ]", "[]");
    }
    const keys = Object.keys(value);
    const entries = keys
      .slice(0, 20)
      .map((key) => `${/^[A-Za-z_$][\w$]*$/.test(key) ? key : JSON.stringify(key)}: ${inspect(value[key], depth + 1)}`);
    if (keys.length > 20) entries.push(`… ${keys.length - 20} more`);
    return entries.length ? `{ ${entries.join(", ")} }` : "{}";
  }

  function shorten(text, max = 120) {
    return text.length > max ? text.slice(0, max - 1) + "…" : text;
  }

  // What a variable holds once it leaves the script: text.
  function storedText(value) {
    if (value === undefined || value === null) return "";
    if (typeof value === "string") return value;
    if (typeof value === "object") {
      try {
        return JSON.stringify(value);
      } catch {
        return String(value);
      }
    }
    return String(value);
  }

  // ---- Console -----------------------------------------------------------

  const consoleObject = {};
  for (const level of ["log", "info", "warn", "error", "debug"]) {
    consoleObject[level] = (...args) => {
      const message = args.map((arg) => (typeof arg === "string" ? arg : inspect(arg))).join(" ");
      out.logs.push({ level: level === "log" ? "info" : level, message, origin });
    };
  }
  globalThis.console = consoleObject;

  // ---- Bytes, UTF-8 and Base64 -------------------------------------------

  function utf8Encode(text) {
    const bytes = [];
    for (const char of String(text)) {
      let code = char.codePointAt(0);
      if (code < 0x80) bytes.push(code);
      else if (code < 0x800) bytes.push(0xc0 | (code >> 6), 0x80 | (code & 63));
      else if (code < 0x10000) bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 63), 0x80 | (code & 63));
      else
        bytes.push(
          0xf0 | (code >> 18),
          0x80 | ((code >> 12) & 63),
          0x80 | ((code >> 6) & 63),
          0x80 | (code & 63),
        );
    }
    return bytes;
  }

  function utf8Decode(bytes) {
    let text = "";
    for (let i = 0; i < bytes.length; ) {
      const byte = bytes[i];
      let code;
      let size;
      if (byte < 0x80) [code, size] = [byte, 1];
      else if (byte >> 5 === 6) [code, size] = [byte & 31, 2];
      else if (byte >> 4 === 14) [code, size] = [byte & 15, 3];
      else if (byte >> 3 === 30) [code, size] = [byte & 7, 4];
      else [code, size] = [0xfffd, 1];
      for (let j = 1; j < size; j++) code = (code << 6) | ((bytes[i + j] ?? 0) & 63);
      text += String.fromCodePoint(code);
      i += size;
    }
    return text;
  }

  const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

  function bytesToBase64(bytes) {
    let text = "";
    for (let i = 0; i < bytes.length; i += 3) {
      const [a, b, c] = [bytes[i], bytes[i + 1], bytes[i + 2]];
      text += B64[a >> 2] + B64[((a & 3) << 4) | ((b ?? 0) >> 4)];
      text += b === undefined ? "=" : B64[((b & 15) << 2) | ((c ?? 0) >> 6)];
      text += c === undefined ? "=" : B64[c & 63];
    }
    return text;
  }

  function base64ToBytes(text) {
    const clean = String(text).replace(/[\s=]/g, "").replace(/-/g, "+").replace(/_/g, "/");
    const bytes = [];
    let buffer = 0;
    let bits = 0;
    for (const char of clean) {
      const value = B64.indexOf(char);
      if (value < 0) throw new Error(`"${char}" is not a Base64 character`);
      buffer = (buffer << 6) | value;
      bits += 6;
      if (bits >= 8) {
        bits -= 8;
        bytes.push((buffer >> bits) & 255);
      }
    }
    return bytes;
  }

  const bytesToHex = (bytes) => bytes.map((b) => b.toString(16).padStart(2, "0")).join("");
  const hexToBytes = (hex) => (String(hex).match(/../g) || []).map((pair) => parseInt(pair, 16));

  globalThis.btoa = (text) => {
    const bytes = [];
    for (const char of String(text)) {
      const code = char.charCodeAt(0);
      if (code > 255) throw new Error("btoa: the string has characters outside Latin-1; encode it to UTF-8 first");
      bytes.push(code);
    }
    return bytesToBase64(bytes);
  };
  globalThis.atob = (text) => String.fromCharCode(...base64ToBytes(text));

  // ---- Crypto ------------------------------------------------------------

  const ALGORITHMS = ["md5", "sha1", "sha256", "sha512"];

  function encodeBytes(bytes, encoding) {
    switch (encoding || "hex") {
      case "hex":
        return bytesToHex(bytes);
      case "base64":
        return bytesToBase64(bytes);
      case "base64url":
        return bytesToBase64(bytes).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
      default:
        throw new Error(`unknown encoding "${encoding}"; use hex, base64 or base64url`);
    }
  }

  function checkAlgorithm(algorithm) {
    const name = String(algorithm).toLowerCase().replace(/-/g, "");
    if (!ALGORITHMS.includes(name)) {
      throw new Error(`unknown algorithm "${algorithm}"; use ${ALGORITHMS.join(", ")}`);
    }
    return name;
  }

  const hashBytes = (algorithm, bytes) => hexToBytes(native.digest(checkAlgorithm(algorithm), bytesToHex(bytes)));
  const hmacBytes = (algorithm, key, bytes) =>
    hexToBytes(native.hmac(checkAlgorithm(algorithm), bytesToHex(key), bytesToHex(bytes)));

  const randomUUID = () => native.uuid();
  globalThis.crypto = { randomUUID };

  // A small CryptoJS-compatible subset, for scripts written against it.
  class WordArray {
    constructor(bytes) {
      this.bytes = bytes;
      this.sigBytes = bytes.length;
    }
    toString(encoder) {
      return (encoder || Hex).stringify(this);
    }
    concat(other) {
      this.bytes = this.bytes.concat(other.bytes);
      this.sigBytes = this.bytes.length;
      return this;
    }
  }
  const Hex = { stringify: (w) => bytesToHex(w.bytes), parse: (s) => new WordArray(hexToBytes(s)) };
  const Base64 = { stringify: (w) => bytesToBase64(w.bytes), parse: (s) => new WordArray(base64ToBytes(s)) };
  const Utf8 = { stringify: (w) => utf8Decode(w.bytes), parse: (s) => new WordArray(utf8Encode(s)) };
  const Latin1 = {
    stringify: (w) => String.fromCharCode(...w.bytes),
    parse: (s) => new WordArray([...String(s)].map((c) => c.charCodeAt(0) & 255)),
  };
  const wordBytes = (data) => (data instanceof WordArray ? data.bytes : utf8Encode(String(data)));
  const CryptoJS = { enc: { Hex, Base64, Utf8, Latin1 }, lib: { WordArray } };
  for (const [name, algorithm] of [
    ["MD5", "md5"],
    ["SHA1", "sha1"],
    ["SHA256", "sha256"],
    ["SHA512", "sha512"],
  ]) {
    CryptoJS[name] = (data) => new WordArray(hashBytes(algorithm, wordBytes(data)));
    CryptoJS["Hmac" + name] = (data, key) => new WordArray(hmacBytes(algorithm, wordBytes(key), wordBytes(data)));
  }

  // ---- Assertions --------------------------------------------------------

  class AssertionError extends Error {
    constructor(message) {
      super(message);
      this.name = "AssertionError";
    }
  }

  function typeOf(value) {
    if (value === null) return "null";
    if (Array.isArray(value)) return "array";
    if (value instanceof Date) return "date";
    if (value instanceof RegExp) return "regexp";
    if (value instanceof Error) return "error";
    if (value instanceof Map) return "map";
    if (value instanceof Set) return "set";
    if (value && typeof value.then === "function") return "promise";
    return typeof value;
  }

  function deepEqual(a, b) {
    if (Object.is(a, b)) return true;
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
    if (Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
    if (a instanceof Date) return a.getTime() === b.getTime();
    if (a instanceof RegExp) return String(a) === String(b);
    if (Array.isArray(a)) return a.length === b.length && a.every((item, i) => deepEqual(item, b[i]));
    if (a instanceof Map || a instanceof Set) return deepEqual([...a], [...b]);
    const keys = Object.keys(a);
    return keys.length === Object.keys(b).length && keys.every((key) => hasOwn(b, key) && deepEqual(a[key], b[key]));
  }

  function sizeOf(value) {
    if (value === null || value === undefined) return undefined;
    if (typeof value === "string" || Array.isArray(value)) return value.length;
    if (value instanceof Map || value instanceof Set) return value.size;
    if (typeof value === "object") return Object.keys(value).length;
    return undefined;
  }

  // A tiny JSON Schema validator: types, enums, numbers, strings, arrays,
  // objects and the combinators. Returns a list of problems.
  function schemaErrors(schema, value, path = "$") {
    if (schema === true || schema === undefined) return [];
    if (schema === false) return [`${path} is not allowed`];
    const errors = [];
    const kind = typeOf(value);
    if (schema.type !== undefined) {
      const types = Array.isArray(schema.type) ? schema.type : [schema.type];
      const matches = types.some((t) => t === kind || (t === "integer" && Number.isInteger(value)) || (t === "number" && kind === "number"));
      if (!matches) return [`${path} should be ${types.join(" or ")} but is ${kind}`];
    }
    if (schema.const !== undefined && !deepEqual(schema.const, value)) errors.push(`${path} should be ${inspect(schema.const)}`);
    if (schema.enum && !schema.enum.some((option) => deepEqual(option, value))) {
      errors.push(`${path} should be one of ${inspect(schema.enum)}`);
    }
    if (kind === "number") {
      if (schema.minimum !== undefined && value < schema.minimum) errors.push(`${path} should be at least ${schema.minimum}`);
      if (schema.maximum !== undefined && value > schema.maximum) errors.push(`${path} should be at most ${schema.maximum}`);
      if (typeof schema.exclusiveMinimum === "number" && value <= schema.exclusiveMinimum) errors.push(`${path} should be above ${schema.exclusiveMinimum}`);
      if (typeof schema.exclusiveMaximum === "number" && value >= schema.exclusiveMaximum) errors.push(`${path} should be below ${schema.exclusiveMaximum}`);
      if (schema.multipleOf && value % schema.multipleOf !== 0) errors.push(`${path} should be a multiple of ${schema.multipleOf}`);
    }
    if (kind === "string") {
      if (schema.minLength !== undefined && value.length < schema.minLength) errors.push(`${path} should have at least ${schema.minLength} characters`);
      if (schema.maxLength !== undefined && value.length > schema.maxLength) errors.push(`${path} should have at most ${schema.maxLength} characters`);
      if (schema.pattern && !new RegExp(schema.pattern, "u").test(value)) errors.push(`${path} should match ${schema.pattern}`);
    }
    if (kind === "array") {
      if (schema.minItems !== undefined && value.length < schema.minItems) errors.push(`${path} should have at least ${schema.minItems} items`);
      if (schema.maxItems !== undefined && value.length > schema.maxItems) errors.push(`${path} should have at most ${schema.maxItems} items`);
      if (schema.uniqueItems && value.some((item, i) => value.findIndex((other) => deepEqual(item, other)) !== i)) {
        errors.push(`${path} should have unique items`);
      }
      if (schema.items && typeof schema.items === "object" && !Array.isArray(schema.items)) {
        value.forEach((item, i) => errors.push(...schemaErrors(schema.items, item, `${path}[${i}]`)));
      }
    }
    if (kind === "object") {
      for (const key of schema.required || []) {
        if (!hasOwn(value, key)) errors.push(`${path}.${key} is required`);
      }
      const properties = schema.properties || {};
      for (const [key, sub] of Object.entries(properties)) {
        if (hasOwn(value, key)) errors.push(...schemaErrors(sub, value[key], `${path}.${key}`));
      }
      if (schema.additionalProperties !== undefined) {
        for (const key of Object.keys(value)) {
          if (!hasOwn(properties, key)) errors.push(...schemaErrors(schema.additionalProperties, value[key], `${path}.${key}`));
        }
      }
    }
    for (const sub of schema.allOf || []) errors.push(...schemaErrors(sub, value, path));
    if (schema.anyOf && !schema.anyOf.some((sub) => schemaErrors(sub, value, path).length === 0)) {
      errors.push(`${path} should match at least one schema in anyOf`);
    }
    if (schema.oneOf && schema.oneOf.filter((sub) => schemaErrors(sub, value, path).length === 0).length !== 1) {
      errors.push(`${path} should match exactly one schema in oneOf`);
    }
    if (schema.not && schemaErrors(schema.not, value, path).length === 0) errors.push(`${path} should not match the schema in not`);
    return errors;
  }

  const WORDS = ["to", "be", "been", "is", "that", "which", "and", "has", "have", "with", "at", "of", "same", "but", "does", "still", "also"];

  class Assertion {
    constructor(actual, message) {
      this._actual = actual;
      this._message = message;
      this._negate = false;
      this._deep = false;
      this._own = false;
      this._any = false;
      this._contains = false;
    }

    _assert(pass, positive, negative) {
      if (this._negate ? pass : !pass) {
        const text = this._negate ? negative : positive;
        throw new AssertionError(this._message ? `${this._message}: ${text}` : text);
      }
      return this;
    }

    _subject() {
      return shorten(inspect(this._actual));
    }

    get not() {
      this._negate = !this._negate;
      return this;
    }
    get deep() {
      this._deep = true;
      return this;
    }
    get own() {
      this._own = true;
      return this;
    }
    get nested() {
      this._nested = true;
      return this;
    }
    get any() {
      this._any = true;
      return this;
    }
    get all() {
      this._any = false;
      return this;
    }

    get ok() {
      return this._assert(!!this._actual, `expected ${this._subject()} to be truthy`, `expected ${this._subject()} to be falsy`);
    }
    get true() {
      return this._assert(this._actual === true, `expected ${this._subject()} to be true`, `expected ${this._subject()} not to be true`);
    }
    get false() {
      return this._assert(this._actual === false, `expected ${this._subject()} to be false`, `expected ${this._subject()} not to be false`);
    }
    get null() {
      return this._assert(this._actual === null, `expected ${this._subject()} to be null`, `expected ${this._subject()} not to be null`);
    }
    get undefined() {
      return this._assert(this._actual === undefined, `expected ${this._subject()} to be undefined`, `expected ${this._subject()} not to be undefined`);
    }
    get NaN() {
      return this._assert(Number.isNaN(this._actual), `expected ${this._subject()} to be NaN`, `expected ${this._subject()} not to be NaN`);
    }
    get exist() {
      const exists = this._actual !== null && this._actual !== undefined;
      return this._assert(exists, `expected ${this._subject()} to exist`, `expected ${this._subject()} not to exist`);
    }
    get exists() {
      return this.exist;
    }
    get empty() {
      const size = sizeOf(this._actual);
      if (size === undefined) throw new AssertionError(`${this._subject()} has no length or size to check`);
      return this._assert(size === 0, `expected ${this._subject()} to be empty`, `expected ${this._subject()} not to be empty`);
    }
    get finite() {
      return this._assert(Number.isFinite(this._actual), `expected ${this._subject()} to be a finite number`, `expected ${this._subject()} not to be a finite number`);
    }
    get include() {
      return this._chainable("include");
    }
    get includes() {
      return this._chainable("include");
    }
    get contain() {
      return this._chainable("include");
    }
    get contains() {
      return this._chainable("include");
    }

    // `include` is both a method and a word that changes later assertions
    // (`.to.include.members(...)`), so it returns a callable assertion.
    _chainable(name) {
      const self = this;
      const callable = function (...args) {
        return self["_" + name](...args);
      };
      self._contains = true;
      Object.setPrototypeOf(callable, self);
      return callable;
    }

    _include(needle, message) {
      if (message) this._message = message;
      const actual = this._actual;
      let pass;
      if (typeof actual === "string") pass = actual.includes(String(needle));
      else if (Array.isArray(actual)) pass = actual.some((item) => (this._deep || typeof needle === "object" ? deepEqual(item, needle) : item === needle));
      else if (actual instanceof Map || actual instanceof Set) pass = actual.has(needle);
      else if (actual && typeof actual === "object" && needle && typeof needle === "object") {
        pass = Object.keys(needle).every((key) => hasOwn(actual, key) && deepEqual(actual[key], needle[key]));
      } else if (actual && typeof actual === "object") pass = hasOwn(actual, needle);
      else throw new AssertionError(`${this._subject()} cannot include anything`);
      return this._assert(pass, `expected ${this._subject()} to include ${shorten(inspect(needle))}`, `expected ${this._subject()} not to include ${shorten(inspect(needle))}`);
    }

    equal(expected, message) {
      if (message) this._message = message;
      const pass = this._deep ? deepEqual(this._actual, expected) : this._actual === expected;
      return this._assert(pass, `expected ${this._subject()} to equal ${shorten(inspect(expected))}`, `expected ${this._subject()} not to equal ${shorten(inspect(expected))}`);
    }
    equals(expected, message) {
      return this.equal(expected, message);
    }
    eq(expected, message) {
      return this.equal(expected, message);
    }
    eql(expected, message) {
      this._deep = true;
      return this.equal(expected, message);
    }
    eqls(expected, message) {
      return this.eql(expected, message);
    }

    _number(op, label, bound, message) {
      if (message) this._message = message;
      const value = this._actual;
      if (typeof value !== "number") throw new AssertionError(`${this._subject()} is not a number`);
      return this._assert(op(value, bound), `expected ${this._subject()} to be ${label} ${bound}`, `expected ${this._subject()} not to be ${label} ${bound}`);
    }
    above(n, m) {
      return this._number((a, b) => a > b, "above", n, m);
    }
    gt(n, m) {
      return this.above(n, m);
    }
    greaterThan(n, m) {
      return this.above(n, m);
    }
    least(n, m) {
      return this._number((a, b) => a >= b, "at least", n, m);
    }
    gte(n, m) {
      return this.least(n, m);
    }
    greaterThanOrEqual(n, m) {
      return this.least(n, m);
    }
    below(n, m) {
      return this._number((a, b) => a < b, "below", n, m);
    }
    lt(n, m) {
      return this.below(n, m);
    }
    lessThan(n, m) {
      return this.below(n, m);
    }
    most(n, m) {
      return this._number((a, b) => a <= b, "at most", n, m);
    }
    lte(n, m) {
      return this.most(n, m);
    }
    lessThanOrEqual(n, m) {
      return this.most(n, m);
    }
    within(low, high, message) {
      if (message) this._message = message;
      const n = this._actual;
      return this._assert(n >= low && n <= high, `expected ${this._subject()} to be within ${low}..${high}`, `expected ${this._subject()} not to be within ${low}..${high}`);
    }
    closeTo(expected, delta, message) {
      if (message) this._message = message;
      return this._assert(Math.abs(this._actual - expected) <= delta, `expected ${this._subject()} to be close to ${expected} ± ${delta}`, `expected ${this._subject()} not to be close to ${expected} ± ${delta}`);
    }
    approximately(expected, delta, message) {
      return this.closeTo(expected, delta, message);
    }

    a(type, message) {
      if (message) this._message = message;
      const wanted = String(type).toLowerCase();
      const actual = typeOf(this._actual);
      const article = /^[aeiou]/.test(wanted) ? "an" : "a";
      return this._assert(actual === wanted, `expected ${this._subject()} to be ${article} ${wanted}`, `expected ${this._subject()} not to be ${article} ${wanted}`);
    }
    an(type, message) {
      return this.a(type, message);
    }
    instanceOf(constructor, message) {
      if (message) this._message = message;
      return this._assert(this._actual instanceof constructor, `expected ${this._subject()} to be an instance of ${constructor.name}`, `expected ${this._subject()} not to be an instance of ${constructor.name}`);
    }
    instanceof(constructor, message) {
      return this.instanceOf(constructor, message);
    }

    property(name, ...rest) {
      const actual = this._actual;
      const label = JSON.stringify(String(name));
      let has;
      let value;
      if (this._nested) {
        const steps = String(name).replace(/\[(\d+)\]/g, ".$1").split(".").filter(Boolean);
        value = actual;
        has = true;
        for (const step of steps) {
          if (value !== null && value !== undefined && step in Object(value)) value = value[step];
          else {
            has = false;
            value = undefined;
            break;
          }
        }
      } else {
        const target = actual === null || actual === undefined ? undefined : Object(actual);
        has = target !== undefined && (this._own ? hasOwn(target, name) : name in target);
        value = has ? target[name] : undefined;
      }
      if (rest.length === 0) {
        this._assert(has, `expected ${this._subject()} to have property ${label}`, `expected ${this._subject()} not to have property ${label}`);
      } else {
        const expected = rest[0];
        const equal = has && (this._deep ? deepEqual(value, expected) : value === expected);
        this._assert(
          equal,
          has
            ? `expected property ${label} to be ${shorten(inspect(expected))} but it is ${shorten(inspect(value))}`
            : `expected ${this._subject()} to have property ${label}`,
          `expected ${this._subject()} not to have property ${label} of ${shorten(inspect(expected))}`,
        );
      }
      if (this._negate) return this;
      const next = new Assertion(value, this._message);
      next._deep = this._deep;
      return next;
    }
    ownProperty(name, ...rest) {
      this._own = true;
      return this.property(name, ...rest);
    }
    haveOwnProperty(name, ...rest) {
      return this.ownProperty(name, ...rest);
    }

    lengthOf(n, message) {
      if (message) this._message = message;
      const size = sizeOf(this._actual);
      if (size === undefined) throw new AssertionError(`${this._subject()} has no length`);
      return this._assert(size === n, `expected ${this._subject()} to have a length of ${n} but got ${size}`, `expected ${this._subject()} not to have a length of ${n}`);
    }
    length(n, message) {
      return this.lengthOf(n, message);
    }

    match(pattern, message) {
      if (message) this._message = message;
      return this._assert(pattern.test(String(this._actual)), `expected ${this._subject()} to match ${pattern}`, `expected ${this._subject()} not to match ${pattern}`);
    }
    matches(pattern, message) {
      return this.match(pattern, message);
    }
    string(text, message) {
      if (message) this._message = message;
      return this._assert(String(this._actual).includes(text), `expected ${this._subject()} to contain ${inspect(text)}`, `expected ${this._subject()} not to contain ${inspect(text)}`);
    }
    oneOf(list, message) {
      if (message) this._message = message;
      const pass = list.some((item) => (this._deep ? deepEqual(item, this._actual) : item === this._actual));
      return this._assert(pass, `expected ${this._subject()} to be one of ${shorten(inspect(list))}`, `expected ${this._subject()} not to be one of ${shorten(inspect(list))}`);
    }
    keys(...wanted) {
      if (wanted.length === 1 && Array.isArray(wanted[0])) wanted = wanted[0];
      else if (wanted.length === 1 && wanted[0] && typeof wanted[0] === "object") wanted = Object.keys(wanted[0]);
      const actual = Object.keys(Object(this._actual));
      let pass;
      if (this._any) pass = wanted.some((key) => actual.includes(key));
      else if (this._contains) pass = wanted.every((key) => actual.includes(key));
      else pass = wanted.length === actual.length && wanted.every((key) => actual.includes(key));
      const how = this._any ? "any of" : this._contains ? "keys including" : "exactly the keys";
      return this._assert(pass, `expected ${this._subject()} to have ${how} ${inspect(wanted)}`, `expected ${this._subject()} not to have ${how} ${inspect(wanted)}`);
    }
    key(...wanted) {
      return this.keys(...wanted);
    }
    members(expected, message) {
      if (message) this._message = message;
      const actual = this._actual;
      const same = (a, b) => (this._deep ? deepEqual(a, b) : a === b);
      const included = expected.every((item) => actual.some((other) => same(item, other)));
      const pass = this._contains ? included : included && actual.length === expected.length;
      const how = this._contains ? "a superset of" : "the same members as";
      return this._assert(pass, `expected ${this._subject()} to have ${how} ${shorten(inspect(expected))}`, `expected ${this._subject()} not to have ${how} ${shorten(inspect(expected))}`);
    }
    satisfy(predicate, message) {
      if (message) this._message = message;
      return this._assert(!!predicate(this._actual), `expected ${this._subject()} to satisfy ${predicate.name || "the predicate"}`, `expected ${this._subject()} not to satisfy ${predicate.name || "the predicate"}`);
    }
    satisfies(predicate, message) {
      return this.satisfy(predicate, message);
    }
    throw(expected, message) {
      if (message) this._message = message;
      let thrown = null;
      try {
        this._actual();
      } catch (error) {
        thrown = error;
      }
      let pass = thrown !== null;
      if (pass && expected !== undefined) {
        const text = thrown && thrown.message !== undefined ? thrown.message : String(thrown);
        if (typeof expected === "string") pass = text.includes(expected);
        else if (expected instanceof RegExp) pass = expected.test(text);
        else if (typeof expected === "function") pass = thrown instanceof expected;
      }
      return this._assert(pass, `expected the function to throw${expected === undefined ? "" : " " + inspect(expected)}`, `expected the function not to throw but it threw ${inspect(thrown)}`);
    }
    throws(expected, message) {
      return this.throw(expected, message);
    }
    Throw(expected, message) {
      return this.throw(expected, message);
    }
    jsonSchema(schema) {
      const problems = schemaErrors(schema, this._actual);
      return this._assert(problems.length === 0, `the value does not match the schema: ${problems.slice(0, 5).join("; ")}`, "expected the value not to match the schema");
    }

    // Response assertions, so `gm.expect(gm.response).to.have.status(200)`
    // works like `gm.response.to.have.status(200)`.
    status(code) {
      return this._response().status(code) && this;
    }
    header(name, value) {
      return this._response().header(name, value) && this;
    }
    jsonBody(...args) {
      return this._response().jsonBody(...args) && this;
    }
    _response() {
      if (!this._actual || !this._actual.__gmResponse) throw new AssertionError(`${this._subject()} is not a response`);
      return responseAssertions(this._actual, this._negate);
    }
  }
  for (const word of WORDS) {
    Object.defineProperty(Assertion.prototype, word, {
      get() {
        return this;
      },
    });
  }

  const expect = (actual, message) => new Assertion(actual, message);

  const assert = (value, message) => {
    if (!value) throw new AssertionError(message || `expected ${inspect(value)} to be truthy`);
  };
  assert.equal = (a, b, m) => expect(a, m).to.equal(b);
  assert.deepEqual = (a, b, m) => expect(a, m).to.eql(b);
  assert.notEqual = (a, b, m) => expect(a, m).to.not.equal(b);
  assert.ok = (a, m) => assert(a, m);

  // ---- Variables ---------------------------------------------------------

  const scopes = {};
  for (const name of ["globals", "collection", "environment", "folder", "request", "data", "session", "local"]) {
    scopes[name] = Object.assign(Object.create(null), (input.scopes && input.scopes[name]) || {});
  }
  const STRONGEST_FIRST = ["local", "session", "data", "request", "folder", "environment", "collection", "globals"];

  function resolve(name) {
    for (const scope of STRONGEST_FIRST) {
      if (name in scopes[scope]) return scopes[scope][name];
    }
    return undefined;
  }

  function interpolate(template, depth = 0, seen = []) {
    return String(template).replace(/\{\{([^{}"\n\r]{1,200})\}\}/g, (whole, raw) => {
      const name = raw.trim();
      if (!name || depth > 10 || seen.includes(name)) return whole;
      let value;
      if (name.startsWith("$env.")) value = native.env(name.slice(5));
      else if (name.startsWith("$")) value = native.dynamic(name.slice(1));
      else value = resolve(name);
      if (value === undefined || value === null) return whole;
      return interpolate(storedText(value), depth + 1, seen.concat(name));
    });
  }

  function makeScope(key, changeScope, extra = {}) {
    const values = scopes[key];
    const record = (name, value) => {
      if (changeScope) out.changes.push({ scope: changeScope, name, value });
    };
    const scope = {
      get: (name) => values[name],
      has: (name) => name in values,
      set(name, value) {
        name = String(name);
        values[name] = value;
        record(name, storedText(value));
        return scope;
      },
      unset(name) {
        name = String(name);
        delete values[name];
        record(name, null);
        return scope;
      },
      clear() {
        for (const name of Object.keys(values)) scope.unset(name);
        return scope;
      },
      toObject: () => Object.assign({}, values),
      replaceIn: (template) => interpolate(template),
    };
    return Object.assign(scope, extra);
  }

  // ---- Lists of headers, query parameters and cookies --------------------

  class PropertyList {
    constructor(items, caseSensitive = true) {
      this._items = items;
      this._caseSensitive = caseSensitive;
    }
    _same(a, b) {
      return this._caseSensitive ? a === b : String(a).toLowerCase() === String(b).toLowerCase();
    }
    _enabled() {
      return this._items.filter((item) => !item.disabled);
    }
    get(key) {
      const item = this._enabled().find((entry) => this._same(entry.key, key));
      return item ? item.value : undefined;
    }
    one(key) {
      return this._enabled().find((entry) => this._same(entry.key, key));
    }
    has(key, value) {
      return this._enabled().some((entry) => this._same(entry.key, key) && (value === undefined || entry.value === value));
    }
    all() {
      return this._items.map((item) => Object.assign({}, item));
    }
    count() {
      return this._items.length;
    }
    each(fn) {
      this._items.forEach((item) => fn(item));
    }
    map(fn) {
      return this._items.map(fn);
    }
    filter(fn) {
      return this._items.filter(fn);
    }
    find(fn) {
      return this._items.find(fn);
    }
    toObject() {
      const object = {};
      for (const item of this._enabled()) object[item.key] = item.value;
      return object;
    }
    _normalize(item, value) {
      if (typeof item === "string") {
        const at = item.indexOf(":");
        return value !== undefined || at < 0
          ? { key: item, value: storedText(value) }
          : { key: item.slice(0, at).trim(), value: item.slice(at + 1).trim() };
      }
      return { key: String(item.key ?? item.name), value: storedText(item.value), disabled: !!item.disabled };
    }
    add(item, value) {
      this._items.push(this._normalize(item, value));
      this._changed();
    }
    upsert(item, value) {
      const entry = this._normalize(item, value);
      const existing = this._items.find((other) => this._same(other.key, entry.key));
      if (existing) Object.assign(existing, entry);
      else this._items.push(entry);
      this._changed();
    }
    remove(predicate) {
      const test = typeof predicate === "function" ? predicate : (item) => this._same(item.key, predicate);
      for (let i = this._items.length - 1; i >= 0; i--) {
        if (test(this._items[i])) this._items.splice(i, 1);
      }
      this._changed();
    }
    clear() {
      this._items.length = 0;
      this._changed();
    }
    toString() {
      return this._enabled().map((item) => `${item.key}: ${item.value}`).join("\n");
    }
    _changed() {}
  }

  // ---- The request -------------------------------------------------------

  function splitQuery(raw) {
    const at = raw.indexOf("?");
    if (at < 0) return [raw, []];
    const hash = raw.indexOf("#", at);
    const query = raw.slice(at + 1, hash < 0 ? undefined : hash);
    const pairs = query
      .split("&")
      .filter((part) => part !== "")
      .map((part) => {
        const eq = part.indexOf("=");
        return eq < 0 ? { key: part, value: null } : { key: part.slice(0, eq), value: part.slice(eq + 1) };
      });
    return [raw.slice(0, at), pairs];
  }

  function makeUrl(raw) {
    let [base, pairs] = splitQuery(raw);
    const query = new PropertyList(pairs);
    query._changed = () => {};
    const url = {
      toString() {
        const items = query._items.filter((item) => !item.disabled);
        return items.length ? `${base}?${items.map((item) => (item.value === null ? item.key : `${item.key}=${item.value}`)).join("&")}` : base;
      },
      update(text) {
        [base, pairs] = splitQuery(String(text));
        query._items = pairs;
      },
      getHost() {
        const match = /^[a-z][\w+.-]*:\/\/([^/?#:]*)/i.exec(base) || /^([^/?#:]*)/.exec(base);
        return match ? match[1] : "";
      },
      getPath() {
        const rest = base.replace(/^[a-z][\w+.-]*:\/\/[^/?#]*/i, "");
        return rest.startsWith("/") ? rest : "/" + rest.split("/").slice(1).join("/");
      },
      getQueryString() {
        return url.toString().split("?")[1] || "";
      },
      query,
      toJSON() {
        return url.toString();
      },
    };
    return url;
  }

  function makeRequest(r) {
    const headers = new PropertyList(
      (r.headers || []).map((h) => ({ key: h.name, value: h.value, disabled: h.enabled === false })),
      false,
    );
    const url = makeUrl(r.url || "");
    let method = r.method || "GET";
    const body = {
      mode: r.body && r.body.mode,
      raw: r.body ? r.body.raw : undefined,
      update(raw) {
        body.raw = typeof raw === "string" ? raw : JSON.stringify(raw);
      },
      toString() {
        return body.raw ?? "";
      },
    };
    return {
      get url() {
        return url;
      },
      set url(value) {
        url.update(String(value));
      },
      get method() {
        return method;
      },
      set method(value) {
        method = String(value).toUpperCase();
      },
      headers,
      body,
      name: input.info && input.info.request_name,
      addHeader: (item, value) => headers.add(item, value),
      upsertHeader: (item, value) => headers.upsert(item, value),
      removeHeader: (key) => headers.remove(key),
      getHeaders: () => headers.toObject(),
      toJSON: () => ({ method, url: url.toString(), headers: headers.all(), body: body.raw }),
      _export: () => ({
        method,
        url: url.toString(),
        headers: headers._items.map((h) => ({ name: h.key, value: storedText(h.value), enabled: !h.disabled })),
        body: body.raw === undefined || body.raw === null ? null : String(body.raw),
      }),
    };
  }

  // ---- The response ------------------------------------------------------

  function parseCookies(headerItems) {
    return headerItems
      .filter((h) => h.key.toLowerCase() === "set-cookie")
      .map((h) => {
        const [pair, ...attributes] = h.value.split(";");
        const eq = pair.indexOf("=");
        const cookie = { key: pair.slice(0, eq).trim(), value: pair.slice(eq + 1).trim(), name: pair.slice(0, eq).trim() };
        for (const attribute of attributes) {
          const [name, ...value] = attribute.split("=");
          cookie[name.trim().toLowerCase()] = value.length ? value.join("=").trim() : true;
        }
        return cookie;
      });
  }

  function makeResponse(r) {
    const headers = new PropertyList((r.headers || []).map((h) => ({ key: h.name, value: h.value })), false);
    const text = r.body || "";
    let parsed;
    const response = {
      __gmResponse: true,
      code: r.code,
      status: r.status,
      reason: () => r.status,
      headers,
      responseTime: r.response_time,
      responseSize: r.response_size,
      cookies: new PropertyList(parseCookies(headers._items)),
      text: () => text,
      json() {
        if (parsed === undefined) {
          try {
            parsed = JSON.parse(text);
          } catch (error) {
            throw new Error(`the response body is not JSON (${error.message}): ${shorten(JSON.stringify(text), 80)}`);
          }
        }
        return parsed;
      },
      get to() {
        return responseAssertions(response, false);
      },
      toJSON: () => ({ code: r.code, status: r.status, headers: headers.all(), body: text }),
    };
    return response;
  }

  function responseAssertions(response, negate) {
    const check = (pass, positive, negative) => {
      if (negate ? pass : !pass) throw new AssertionError(negate ? negative : positive);
      return api;
    };
    const statusIn = (low, high, label) => {
      const code = response.code;
      return check(code >= low && code <= high, `expected the response to be ${label} but the status is ${code}`, `expected the response not to be ${label} but the status is ${code}`);
    };
    const codeIs = (expected, label) =>
      check(response.code === expected, `expected the response to be ${label} (${expected}) but the status is ${response.code}`, `expected the response not to be ${label} (${expected})`);
    const contentType = () => String(response.headers.get("content-type") || "");
    const api = {
      get not() {
        return responseAssertions(response, !negate);
      },
      status(expected) {
        if (typeof expected === "number") {
          return check(response.code === expected, `expected the status to be ${expected} but it is ${response.code}`, `expected the status not to be ${expected}`);
        }
        return check(
          String(response.status).toLowerCase() === String(expected).toLowerCase(),
          `expected the status to be "${expected}" but it is "${response.status}"`,
          `expected the status not to be "${expected}"`,
        );
      },
      header(name, value) {
        const actual = response.headers.get(name);
        if (value === undefined) {
          return check(actual !== undefined, `expected the response to have a ${name} header`, `expected the response not to have a ${name} header`);
        }
        return check(actual === value, `expected the ${name} header to be ${inspect(value)} but it is ${inspect(actual)}`, `expected the ${name} header not to be ${inspect(value)}`);
      },
      body(expected) {
        const text = response.text();
        if (expected === undefined) return check(text.length > 0, "expected the response to have a body", "expected the response to have no body");
        if (expected instanceof RegExp) return check(expected.test(text), `expected the body to match ${expected}`, `expected the body not to match ${expected}`);
        if (typeof expected === "object") {
          return check(deepEqual(response.json(), expected), `expected the JSON body to equal ${shorten(inspect(expected))}`, `expected the JSON body not to equal ${shorten(inspect(expected))}`);
        }
        return check(text === String(expected), `expected the body to be ${shorten(inspect(expected))}`, `expected the body not to be ${shorten(inspect(expected))}`);
      },
      jsonBody(...args) {
        let json;
        try {
          json = JSON.parse(response.text());
        } catch {
          return check(false, "expected the response body to be JSON", "");
        }
        if (args.length === 0) return check(true, "", "expected the response body not to be JSON");
        if (typeof args[0] === "object" && args[0] !== null) {
          return check(deepEqual(json, args[0]), `expected the JSON body to equal ${shorten(inspect(args[0]))}`, `expected the JSON body not to equal ${shorten(inspect(args[0]))}`);
        }
        const path = String(args[0]).split(".").filter(Boolean);
        let value = json;
        let found = true;
        for (const key of path) {
          if (value !== null && typeof value === "object" && key in value) value = value[key];
          else {
            found = false;
            break;
          }
        }
        if (args.length === 1) return check(found, `expected the JSON body to have ${args[0]}`, `expected the JSON body not to have ${args[0]}`);
        return check(found && deepEqual(value, args[1]), `expected ${args[0]} to be ${inspect(args[1])} but it is ${found ? inspect(value) : "missing"}`, `expected ${args[0]} not to be ${inspect(args[1])}`);
      },
      jsonSchema(schema) {
        const problems = schemaErrors(schema, response.json());
        return check(problems.length === 0, `the body does not match the schema: ${problems.slice(0, 5).join("; ")}`, "expected the body not to match the schema");
      },
      get ok() {
        return statusIn(200, 299, "successful");
      },
      get success() {
        return statusIn(200, 299, "successful");
      },
      get info() {
        return statusIn(100, 199, "informational");
      },
      get redirection() {
        return statusIn(300, 399, "a redirection");
      },
      get error() {
        return statusIn(400, 599, "an error");
      },
      get clientError() {
        return statusIn(400, 499, "a client error");
      },
      get serverError() {
        return statusIn(500, 599, "a server error");
      },
      get accepted() {
        return codeIs(202, "accepted");
      },
      get badRequest() {
        return codeIs(400, "a bad request");
      },
      get unauthorized() {
        return codeIs(401, "unauthorized");
      },
      get forbidden() {
        return codeIs(403, "forbidden");
      },
      get notFound() {
        return codeIs(404, "not found");
      },
      get rateLimited() {
        return codeIs(429, "rate limited");
      },
      get withBody() {
        return api.body();
      },
      get json() {
        let parses = true;
        try {
          JSON.parse(response.text());
        } catch {
          parses = false;
        }
        return check(parses && /json/i.test(contentType()), `expected a JSON response but the content type is "${contentType()}"`, "expected the response not to be JSON");
      },
      get html() {
        return check(/html/i.test(contentType()), `expected an HTML response but the content type is "${contentType()}"`, "expected the response not to be HTML");
      },
      get xml() {
        return check(/xml/i.test(contentType()), `expected an XML response but the content type is "${contentType()}"`, "expected the response not to be XML");
      },
    };
    for (const word of WORDS) {
      Object.defineProperty(api, word, { get: () => api });
    }
    return api;
  }

  // ---- Tests -------------------------------------------------------------

  function errorText(error) {
    if (error instanceof Error) return error.message || String(error);
    return inspect(error);
  }

  function test(name, fn) {
    const result = { name: String(name), passed: true };
    out.tests.push(result);
    const fail = (error) => {
      result.passed = false;
      result.message = errorText(error);
    };
    try {
      const returned = typeof fn === "function" ? fn() : undefined;
      if (returned && typeof returned.then === "function") returned.then(undefined, fail);
    } catch (error) {
      fail(error);
    }
    return gm;
  }
  test.skip = (name) => {
    out.tests.push({ name: String(name), passed: true, skipped: true });
    return gm;
  };

  // ---- Sending requests from scripts ------------------------------------

  function toOutgoing(request) {
    if (typeof request === "string") request = { url: request };
    const rawHeaders = request.header || request.headers || [];
    const headers = Array.isArray(rawHeaders)
      ? rawHeaders.filter((h) => !h.disabled).map((h) => (typeof h === "string" ? { name: h.split(":")[0].trim(), value: h.split(":").slice(1).join(":").trim() } : { name: String(h.key ?? h.name), value: storedText(h.value) }))
      : Object.entries(rawHeaders).map(([name, value]) => ({ name, value: storedText(value) }));
    let body = { type: "none" };
    const source = request.body;
    if (typeof source === "string") body = { type: "bytes", data: { text: source } };
    else if (source && source.mode === "urlencoded") {
      const items = Array.isArray(source.urlencoded) ? source.urlencoded : Object.entries(source.urlencoded || {}).map(([key, value]) => ({ key, value }));
      const encoded = items
        .filter((item) => !item.disabled)
        .map((item) => `${encodeURIComponent(item.key)}=${encodeURIComponent(storedText(item.value))}`)
        .join("&");
      body = { type: "bytes", data: { text: encoded } };
      if (!headers.some((h) => h.name.toLowerCase() === "content-type")) headers.push({ name: "Content-Type", value: "application/x-www-form-urlencoded" });
    } else if (source && source.raw !== undefined) {
      body = { type: "bytes", data: { text: typeof source.raw === "string" ? source.raw : JSON.stringify(source.raw) } };
    } else if (source && typeof source === "object" && !source.mode) {
      body = { type: "bytes", data: { text: JSON.stringify(source) } };
      if (!headers.some((h) => h.name.toLowerCase() === "content-type")) headers.push({ name: "Content-Type", value: "application/json" });
    }
    const url = typeof request.url === "string" ? request.url : request.url && request.url.toString ? request.url.toString() : "";
    return { method: String(request.method || "GET").toUpperCase(), url, headers, body };
  }

  function sendRequest(request, callback) {
    const reply = JSON.parse(native.send(JSON.stringify(toOutgoing(request))));
    const error = reply.error ? new Error(reply.error) : null;
    const response = reply.response ? makeResponse(reply.response) : null;
    if (typeof callback === "function") {
      callback(error, response);
      return undefined;
    }
    return error ? Promise.reject(error) : Promise.resolve(response);
  }

  // ---- gm ----------------------------------------------------------------

  const info = input.info || {};
  const gm = {
    info: {
      eventName: input.event === "pre_request" ? "prerequest" : "test",
      iteration: info.iteration || 0,
      iterationCount: info.iteration_count || 1,
      requestName: info.request_name || "",
      requestId: info.request_path || "",
    },
    environment: makeScope("environment", "environment", { name: (input.scopes && input.scopes.environment_name) || undefined }),
    collectionVariables: makeScope("collection", "collection"),
    globals: makeScope("globals", "global"),
    iterationData: makeScope("data", null),
    session: makeScope("session", "session"),
    variables: {
      get: (name) => resolve(name),
      has: (name) => resolve(name) !== undefined,
      set(name, value) {
        scopes.local[String(name)] = value;
        out.changes.push({ scope: "local", name: String(name), value: storedText(value) });
      },
      unset(name) {
        delete scopes.local[String(name)];
        out.changes.push({ scope: "local", name: String(name), value: null });
      },
      clear() {
        for (const name of Object.keys(scopes.local)) {
          delete scopes.local[name];
          out.changes.push({ scope: "local", name, value: null });
        }
      },
      toObject() {
        const object = {};
        for (const scope of STRONGEST_FIRST.slice().reverse()) Object.assign(object, scopes[scope]);
        return object;
      },
      replaceIn: (template) => interpolate(template),
    },
    request: makeRequest(input.request || {}),
    response: input.response ? makeResponse(input.response) : undefined,
    cookies: new PropertyList((input.cookies || []).map((c) => ({ key: c.name, value: c.value, domain: c.domain, path: c.path }))),
    test,
    expect,
    sendRequest,
    execution: {
      setNextRequest(name) {
        out.next_request = name === null || name === undefined ? null : String(name);
      },
      skipRequest() {
        out.skip_request = true;
      },
      location: [info.request_path || ""],
    },
    crypto: {
      hash: (algorithm, data, encoding) => encodeBytes(hashBytes(algorithm, utf8Encode(storedText(data))), encoding),
      hmac: (algorithm, key, data, encoding) => encodeBytes(hmacBytes(algorithm, utf8Encode(storedText(key)), utf8Encode(storedText(data))), encoding),
      randomUUID,
      base64Encode: (text) => bytesToBase64(utf8Encode(storedText(text))),
      base64Decode: (text) => utf8Decode(base64ToBytes(text)),
    },
  };
  globalThis.gm = gm;
  globalThis.pm = gm;
  globalThis.expect = expect;

  // The older global API that some imported scripts still use.
  const legacyTests = {};
  globalThis.tests = legacyTests;
  globalThis.postman = {
    setNextRequest: gm.execution.setNextRequest,
    setEnvironmentVariable: (k, v) => gm.environment.set(k, v),
    getEnvironmentVariable: (k) => gm.environment.get(k),
    clearEnvironmentVariable: (k) => gm.environment.unset(k),
    setGlobalVariable: (k, v) => gm.globals.set(k, v),
    getGlobalVariable: (k) => gm.globals.get(k),
    clearGlobalVariable: (k) => gm.globals.unset(k),
  };
  if (gm.response) {
    globalThis.responseBody = gm.response.text();
    globalThis.responseCode = { code: gm.response.code, name: gm.response.status, detail: gm.response.status };
    globalThis.responseTime = gm.response.responseTime;
    globalThis.responseHeaders = gm.response.headers.toObject();
  }

  // ---- Modules -----------------------------------------------------------

  const modules = {
    "crypto-js": CryptoJS,
    uuid: { v4: randomUUID },
    chai: { expect, assert, AssertionError },
    btoa: globalThis.btoa,
    atob: globalThis.atob,
  };
  globalThis.require = (name) => {
    if (hasOwn(modules, name)) return modules[name];
    throw new Error(`require("${name}") is not available in Getman scripts; available modules: ${Object.keys(modules).join(", ")}`);
  };

  // ---- Timers ------------------------------------------------------------
  //
  // Scripts run to completion before the request continues, so timers run
  // on a virtual clock: callbacks fire in order without real waiting.

  let clock = 0;
  let nextTimer = 1;
  let timers = [];
  const addTimer = (fn, ms, args, interval) => {
    const id = nextTimer++;
    const delay = Math.max(0, Number(ms) || 0);
    timers.push({ id, at: clock + delay, fn, args, interval: interval ? Math.max(1, delay) : 0 });
    return id;
  };
  globalThis.setTimeout = (fn, ms, ...args) => addTimer(fn, ms, args, false);
  globalThis.setInterval = (fn, ms, ...args) => addTimer(fn, ms, args, true);
  globalThis.clearTimeout = globalThis.clearInterval = (id) => {
    timers = timers.filter((timer) => timer.id !== id);
  };
  globalThis.setImmediate = (fn, ...args) => addTimer(fn, 0, args, false);

  // ---- Hooks for Rust ----------------------------------------------------

  globalThis.__gm_begin = (name) => {
    origin = name;
  };
  globalThis.__gm_tick = () => {
    if (timers.length === 0) return false;
    timers.sort((a, b) => a.at - b.at || a.id - b.id);
    const timer = timers.shift();
    clock = timer.at;
    if (timer.interval) timers.push(Object.assign({}, timer, { at: clock + timer.interval }));
    try {
      timer.fn(...timer.args);
    } catch (error) {
      consoleObject.error(`Uncaught in timer: ${errorText(error)}`);
    }
    return true;
  };
  globalThis.__gm_finish = () => {
    for (const [name, passed] of Object.entries(legacyTests)) {
      out.tests.push(passed ? { name, passed: true } : { name, passed: false, message: "the test's value is falsy" });
    }
    if (input.event === "pre_request") out.request = gm.request._export();
    if (out.next_request === undefined) delete out.next_request;
    return JSON.stringify(out);
  };
  globalThis.__gm_describe = (error) => errorText(error);
})();
