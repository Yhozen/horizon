#!/usr/bin/env node
// Real input for a running browser session (the open edition's proof recordings,
// docs/open-creative.md): held keys with their physical codes and relative mouse
// motion, sent through the Chrome DevTools Protocol's Input domain, which
// delivers them as the browser's own input events. agent-browser drives
// everything else; `agent-browser get cdp-url` names the browser.
//
//   node tools/web/cdp_input.mjs CDP_URL STEP...
//
// Steps: down:CODE  up:CODE  tap:CODE  hold:CODE:MS  wait:MS  look:DX:DY  move:X:Y  click:left|right|middle
//        window:W:H (place the browser window at the screen's top left, W x H)
//        type:TEXT (key presses carrying text: letters, digits, space and /)
// The mouse position persists between runs (CDP_INPUT_STATE, default in the temp
// folder), so relative look motion under pointer lock never jumps.
// CODE is a KeyboardEvent.code: KeyW, Space, ShiftLeft, ControlLeft, Digit1, Escape...
import { readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [url, ...steps] = process.argv.slice(2);
if (!url || !steps.length) {
  console.error("usage: cdp_input.mjs CDP_URL STEP...");
  process.exit(2);
}
const socket = new WebSocket(url);
let next = 1;
const pending = new Map();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    const { resolve, reject } = pending.get(message.id);
    pending.delete(message.id);
    message.error ? reject(new Error(message.error.message)) : resolve(message.result);
  }
});
const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
  const id = next++;
  pending.set(id, { resolve, reject });
  socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
});
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve);
  socket.addEventListener("error", reject);
});
const { targetInfos } = await send("Target.getTargets");
// The game's page: the newest page target with an http(s) URL (a recording
// opens its own page context).
const pages = targetInfos.filter((t) => t.type === "page" && /^https?:/.test(t.url));
const anyPage = targetInfos.filter((t) => t.type === "page" && !t.url.startsWith("devtools"));
const page = pages[pages.length - 1] ?? anyPage[anyPage.length - 1];
if (!page) throw new Error("no page in this browser");
const { sessionId } = await send("Target.attachToTarget", { targetId: page.targetId, flatten: true });
const input = (method, params) => send(method, params, sessionId);
const KEYS = {
  Space: [" ", 32], ShiftLeft: ["Shift", 16], ControlLeft: ["Control", 17], Escape: ["Escape", 27],
  Enter: ["Enter", 13], Tab: ["Tab", 9], F5: ["F5", 116],
};
function key(code) {
  if (KEYS[code]) return KEYS[code];
  if (/^Key[A-Z]$/.test(code)) return [code.slice(3).toLowerCase(), code.charCodeAt(3)];
  if (/^Digit[0-9]$/.test(code)) return [code.slice(5), code.charCodeAt(5)];
  throw new Error(`unknown key code ${code}`);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const statePath = process.env.CDP_INPUT_STATE ?? join(tmpdir(), "cdp-input-mouse.json");
let mouse = { x: 660, y: 420 };
try { mouse = JSON.parse(readFileSync(statePath, "utf8")); } catch { /* first run */ }
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
async function keyEvent(type, code) {
  const [k, keyCode] = key(code);
  await input("Input.dispatchKeyEvent", { type: type === "down" ? "rawKeyDown" : "keyUp", code, key: k,
    windowsVirtualKeyCode: keyCode, nativeVirtualKeyCode: keyCode });
}
for (const step of steps) {
  const [kind, a, b] = step.split(":");
  if (kind === "down" || kind === "up") await keyEvent(kind, a);
  else if (kind === "tap") { await keyEvent("down", a); await sleep(200); await keyEvent("up", a); }
  else if (kind === "hold") { await keyEvent("down", a); await sleep(Number(b)); await keyEvent("up", a); }
  else if (kind === "wait") await sleep(Number(a));
  else if (kind === "look") {
    // Relative motion in small steps, as a mouse sends it under pointer lock.
    const steps = 12, dx = Number(a) / steps, dy = Number(b) / steps;
    for (let i = 0; i < steps; i++) {
      // Inside the window: positions past its edge would be dropped.
      mouse = { x: clamp(mouse.x + dx, 30, 1410), y: clamp(mouse.y + dy, 30, 970) };
      await input("Input.dispatchMouseEvent", { type: "mouseMoved", x: mouse.x, y: mouse.y });
      await sleep(30);
    }
  } else if (kind === "type") {
    for (const ch of step.slice("type:".length)) {
      const code = /[a-z]/i.test(ch) ? `Key${ch.toUpperCase()}` : /[0-9]/.test(ch) ? `Digit${ch}`
        : ch === " " ? "Space" : ch === "/" ? "Slash" : ch === "@" ? "Digit2" : null;
      if (!code) throw new Error(`cannot type ${ch}`);
      const keyCode = ch === "/" ? 191 : ch === " " ? 32 : ch.toUpperCase().charCodeAt(0);
      await input("Input.dispatchKeyEvent", { type: "keyDown", code, key: ch, text: ch, unmodifiedText: ch,
        windowsVirtualKeyCode: keyCode, nativeVirtualKeyCode: keyCode });
      await sleep(60);
      await input("Input.dispatchKeyEvent", { type: "keyUp", code, key: ch, windowsVirtualKeyCode: keyCode,
        nativeVirtualKeyCode: keyCode });
      await sleep(60);
    }
  } else if (kind === "window") {
    const { windowId } = await send("Browser.getWindowForTarget", { targetId: page.targetId });
    await send("Browser.setWindowBounds", { windowId, bounds: { windowState: "normal" } });
    await send("Browser.setWindowBounds", { windowId, bounds: { left: 0, top: 0, width: Number(a), height: Number(b) } });
  } else if (kind === "move") {
    mouse = { x: Number(a), y: Number(b) };
    await input("Input.dispatchMouseEvent", { type: "mouseMoved", x: mouse.x, y: mouse.y });
  } else if (kind === "click") {
    const button = a ?? "left";
    await input("Input.dispatchMouseEvent", { type: "mousePressed", x: mouse.x, y: mouse.y, button, clickCount: 1 });
    await sleep(90);
    await input("Input.dispatchMouseEvent", { type: "mouseReleased", x: mouse.x, y: mouse.y, button, clickCount: 1 });
  } else throw new Error(`unknown step ${step}`);
}
writeFileSync(statePath, JSON.stringify(mouse));
socket.close();
