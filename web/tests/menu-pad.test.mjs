import test from "node:test";
import assert from "node:assert/strict";
import { padOf, vendorOf } from "../menu/pad.ts";

test("the menu names a pad the way the game does, with its own face buttons", () => {
  const chrome = "DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)";
  assert.equal(vendorOf(chrome), 0x054c);
  assert.deepEqual(padOf(chrome), { name: "DualSense Wireless Controller", confirm: "✕", back: "○" });
  assert.deepEqual(padOf("054c-0ce6-DualSense Wireless Controller"), { name: "DualSense Wireless Controller", confirm: "✕", back: "○" });
  assert.deepEqual(padOf("Xbox 360 Controller (XInput STANDARD GAMEPAD)"), { name: "Xbox 360 Controller", confirm: "A", back: "B" });
  assert.deepEqual(padOf("057e-2009-Pro Controller"), { name: "Pro Controller", confirm: "B", back: "A" });
  assert.equal(vendorOf("Xbox Wireless Controller"), undefined);
  assert.equal(padOf("").name, "Controller");
});
