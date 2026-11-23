import test from "node:test";
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { watchFrameRate } from "../scripts/bench-frames.mjs";

function playing(t, chunks, options) {
  let time = 0;
  t.mock.method(performance, "now", () => time);
  const output = [];
  t.mock.method(console, "log", (line) => output.push(line));
  const page = new EventEmitter();
  const report = watchFrameRate(page, { tail: 2, ...options });
  const log = (text) => page.emit("console", { text: () => text });
  for (const frame of [10, 30, 50]) {
    time += 2000;
    log(`Horizon web: frame ${frame} screen Playing title false paused false stages [] chunks ${chunks} generated ${chunks} meshed 504 loading false entry false player Some([0.5, 69, 0.5])`);
  }
  return { page, report, log, output };
}

test("the full radius-four workload reports steady FPS", (t) => {
  const { report, output } = playing(t, 81, { expectedChunks: "81" });
  report();
  assert.equal(output.at(-1), "steady fps (last 2 intervals): 10.00");
});

test("a smaller but settled world does not count", (t) => {
  const { report } = playing(t, 29, { expectedChunks: "81" });
  assert.throws(report, /Expected 81 loaded chunks, got 29/);
});

test("strict mode rejects a game stop after the last good sample", (t) => {
  const { report, log } = playing(t, 81, { strict: true });
  log("[ERROR] horizon_client::game: Horizon: the game stopped: frame-limited run timed out");
  assert.throws(report, /frame-limited run timed out/);
});

test("an uncaught page error invalidates an otherwise complete scene", (t) => {
  const { report, page } = playing(t, 81, { expectedChunks: "81" });
  page.emit("pageerror", new Error("render failed"));
  assert.throws(report, /render failed/);
});
