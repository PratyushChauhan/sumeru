import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { resolveNativeArgs } from "./args.js";

describe("resolveNativeArgs", () => {
  it("defaults bare sumeru to mcp-stdio", () => {
    assert.deepEqual(resolveNativeArgs([]), ["mcp-stdio"]);
  });

  it("passes through mcp-stdio and trailing args", () => {
    assert.deepEqual(resolveNativeArgs(["mcp-stdio"]), ["mcp-stdio"]);
    assert.deepEqual(resolveNativeArgs(["mcp-stdio", "--foo"]), [
      "mcp-stdio",
      "--foo",
    ]);
  });

  it("strips gui for desktop launch", () => {
    assert.deepEqual(resolveNativeArgs(["gui"]), []);
    assert.deepEqual(resolveNativeArgs(["gui", "--hidden"]), ["--hidden"]);
  });

  it("passes other argv through unchanged", () => {
    assert.deepEqual(resolveNativeArgs(["--hidden"]), ["--hidden"]);
    assert.deepEqual(resolveNativeArgs(["doctor"]), ["doctor"]);
  });
});
