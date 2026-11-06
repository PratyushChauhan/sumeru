import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { agentServerName, keyAllows, scopeIds, scopeLabel } from "./keys.js";

const servers = [{ id: "a" }, { id: "b" }, { id: "c" }];

describe("API key helpers", () => {
  it("mirrors Rust scope rules", () => {
    assert.equal(keyAllows({ mode: "exclude", mcp_ids: [] }, "a"), true);
    assert.equal(keyAllows({ mode: "exclude", mcp_ids: ["a"] }, "a"), false);
    assert.equal(keyAllows({ mode: "include", mcp_ids: ["a"] }, "a"), true);
    assert.equal(keyAllows({ mode: "include", mcp_ids: ["a"] }, "b"), false);
  });

  it("stores the right ids for each mode", () => {
    assert.deepEqual(scopeIds("include", ["a"], servers), ["a"]);
    assert.deepEqual(scopeIds("exclude", ["a"], servers), ["b", "c"]);
    assert.deepEqual(scopeIds("exclude", ["a", "b", "c"], servers), []);
  });

  it("summarises scope", () => {
    assert.equal(scopeLabel({ mode: "exclude", mcp_ids: [] }, servers), "All MCPs");
    assert.equal(scopeLabel({ mode: "exclude", mcp_ids: ["a"] }, servers), "All except 1");
    assert.equal(scopeLabel({ mode: "include", mcp_ids: ["a", "b"] }, servers), "Only 2 of 3 MCPs");
    assert.equal(scopeLabel({ mode: "include", mcp_ids: [] }, servers), "No MCPs");
  });

  it("names agent servers per key", () => {
    assert.equal(agentServerName({ id: "default", name: "Default" }), "sumeru");
    assert.equal(agentServerName({ id: "x", name: "Work · Acme!" }), "sumeru-work-acme");
    assert.equal(agentServerName({ id: "1234567890", name: "···" }), "sumeru-12345678");
  });
});
