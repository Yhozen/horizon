import test from "node:test";
import assert from "node:assert/strict";
import { openMinecraftPort } from "../src/multiplayer-port.ts";

test("cold container retries synchronous connect errors, failed opens, and throwing close methods", async () => {
  let attempts = 0, starts = 0, closes = 0;
  const ready = { opened: Promise.resolve(), closed: Promise.resolve() };
  const probe = {
    ...ready, async close() {},
    writable: new WritableStream({ write(bytes) { assert.equal(bytes.byteLength, 19); } }),
    readable: new ReadableStream({ start(controller) { controller.enqueue(new Uint8Array([10, 0])); controller.close(); } }),
  };
  const container = {
    running: false, start(options) { assert.equal(options.enableInternet, true); starts++; this.running = true; },
    async setInactivityTimeout(timeout) { assert.equal(timeout, 300_000); },
    getTcpPort(port) {
      assert.equal(port, 25565);
      return { connect() {
        attempts++;
        if (attempts === 1) throw new Error("container not listening");
        if (attempts === 2) return {
          opened: Promise.reject(new Error("not ready")), closed: Promise.reject(new Error("not ready")),
          close() { closes++; throw new Error("not listening"); },
        };
        return attempts === 3 ? probe : ready;
      } };
    },
  };
  assert.equal(await openMinecraftPort(container, 3, 0), ready);
  assert.equal(starts, 1); assert.equal(attempts, 4); assert.equal(closes, 1);
});

test("a resolved opened promise is insufficient until the status exchange succeeds", async () => {
  let attempts = 0;
  const ready = { opened: Promise.resolve(), closed: Promise.resolve() };
  const container = {
    running: true, async setInactivityTimeout() {},
    getTcpPort() { return { connect() {
      attempts++;
      if (attempts === 3) return ready;
      return {
        ...ready, async close() {},
        writable: new WritableStream({ write() { if (attempts === 1) throw new Error("not listening"); } }),
        readable: new ReadableStream({ start(controller) { controller.enqueue(new Uint8Array([10, 0])); controller.close(); } }),
      };
    } }; },
  };
  assert.equal(await openMinecraftPort(container, 3, 0), ready);
  assert.equal(attempts, 3);
});

test("an existing server is reused and failed startup retries are bounded", async () => {
  let attempts = 0;
  const container = {
    running: true, start() { assert.fail("must reuse the running server"); },
    async setInactivityTimeout() {},
    getTcpPort() { return { connect() { attempts++; throw new Error("still starting"); } }; },
  };
  await assert.rejects(openMinecraftPort(container, 3, 0), /startup timed out.*still starting/);
  assert.equal(attempts, 3);
});
