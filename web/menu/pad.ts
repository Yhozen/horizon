// Controllers on the menu: which pad the page sees, and its ✕ / A and ○ / B presses, read
// from the Gamepad API once per frame. The game reads pads itself once it starts
// (crates/horizon-client/src/game/pad/browser.rs); this only lives while the menu shows.

/** A connected pad: its name without the ids the browser adds, and what its bottom face button says. */
export type Pad = { name: string; confirm: string; back: string };

export type PadEvent =
  | { _tag: "Changed"; pad: Pad | undefined }
  | { _tag: "Pressed"; button: "confirm" | "back" };

/** The USB vendor in a `Gamepad.id`: Chrome's `… (… Vendor: 054c Product: 0ce6)` or Firefox's `054c-0ce6-…`. */
export function vendorOf(id: string): number | undefined {
  const hex = /Vendor: ([0-9a-f]{4})/i.exec(id)?.[1] ?? /^([0-9a-f]{4})-[0-9a-f]{4}-/i.exec(id)?.[1];
  return hex === undefined ? undefined : parseInt(hex, 16);
}

const FACES = new Map<number, [string, string]>([[0x054c, ["✕", "○"]], [0x057e, ["B", "A"]]]);

/** What the player sees for a pad (the same reading as gamepad::browser_name in the game). */
export function padOf(id: string): Pad {
  const name = id.replace(/\s*\((?:[^()]*(?:Vendor:|GAMEPAD)[^()]*)\)\s*$/, "").replace(/^[0-9a-f]{4}-[0-9a-f]{4}-/i, "").trim();
  // The standard mapping is positional: the bottom button is ✕ on PlayStation, B on Nintendo.
  const [confirm, back] = FACES.get(vendorOf(id) ?? 0) ?? ["A", "B"];
  return { name: name || "Controller", confirm, back };
}

const frame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

/** The pads' changes and presses until the consumer stops. A button already held when a pad
 *  appears (the press that showed it to the page) waits for its release. */
export async function* padEvents(): AsyncGenerator<PadEvent> {
  if (typeof navigator.getGamepads !== "function") return;
  let shown = "";
  let held = new Map<number, [boolean, boolean]>();
  while (true) {
    await frame();
    let pads: Gamepad[] = [];
    try { pads = navigator.getGamepads().filter((p): p is Gamepad => p !== null && p.connected); } catch {}
    const ids = pads.map((p) => p.id).join("\n");
    if (ids !== shown) {
      shown = ids;
      yield { _tag: "Changed", pad: pads[0] && padOf(pads[0].id) };
    }
    const next = new Map<number, [boolean, boolean]>();
    for (const pad of pads) {
      const now: [boolean, boolean] = pad.mapping === "standard"
        ? [Boolean(pad.buttons[0]?.pressed), Boolean(pad.buttons[1]?.pressed)] : [false, false];
      const was = held.get(pad.index) ?? now;
      next.set(pad.index, now);
      if (now[0] && !was[0]) yield { _tag: "Pressed", button: "confirm" };
      if (now[1] && !was[1]) yield { _tag: "Pressed", button: "back" };
    }
    held = next;
  }
}
