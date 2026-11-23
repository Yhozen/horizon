// The landing menu of both sites (horizon.rccpr.tech and open.rccpr.tech): the player
// names themselves, chooses a world, reads how to play, and only then loads the game.
// A Foldkit program (The Elm Architecture on Effect): one Model, Messages as facts,
// a pure update, side effects as Commands, and the game's loading as a Subscription
// that lives while the Model says it is loading.
import { Effect, Schema, Stream } from "effect";
import { Command, Subscription, type Runtime, type Update } from "foldkit";
import type { Document, Html, HtmlBuilder } from "foldkit/html";
import { defineMessageUnion } from "foldkit/message";
import { defineTaggedUnion } from "foldkit/schema";
import { modifyFields } from "foldkit/struct";
import { EDITIONS, EditionId, NAME, WorldId, type Edition, type World } from "./edition.ts";
import { boot, STEPS, type Progress } from "./game.ts";
import { padEvents, type PadEvent } from "./pad.ts";

// MODEL

export const Phase = defineTaggedUnion({
  Menu: {},
  Loading: { step: Schema.String, index: Schema.Number },
  Playing: {},
  Failed: { error: Schema.String, reload: Schema.Boolean },
});
export type Phase = typeof Phase.Type;

const Choice = Schema.Literals(["diagnostics", "recording"]);
const Pad = Schema.Struct({ name: Schema.String, confirm: Schema.String, back: Schema.String });
const CreditLine = Schema.Struct({ text: Schema.String, href: Schema.String });
type CreditLine = typeof CreditLine.Type;

export const Model = Schema.Struct({
  edition: EditionId,
  name: Schema.String,
  world: WorldId,
  /** The multiplayer worlds this deployment serves (/worlds.json); single player always works. */
  available: Schema.Array(WorldId),
  checkedWorlds: Schema.Boolean,
  sheet: Schema.Boolean,
  fullscreen: Schema.Boolean,
  /** The controller the page sees, once one of its buttons has been pressed. */
  pad: Schema.NullOr(Pad),
  phase: Phase,
  /** The page's ?args=, for development: start at once with these game arguments. */
  custom: Schema.String,
  telemetry: Schema.Struct({ available: Schema.Boolean, diagnostics: Schema.Boolean, recording: Schema.Boolean }),
  credits: Schema.Literals(["Idle", "Loading", "Loaded", "Failed"]),
  creditLines: Schema.Array(CreditLine),
});
export type Model = typeof Model.Type;

// MESSAGE

export const Message = defineMessageUnion({
  ChangedName: { value: Schema.String },
  ChoseWorld: { world: WorldId },
  ToggledSheet: {},
  ClosedSheet: {},
  ClickedFullscreen: {},
  ChangedFullscreen: { on: Schema.Boolean },
  SubmittedPlay: {},
  ChangedPad: { pad: Schema.NullOr(Pad) },
  PressedPadBack: {},
  ClickedCancel: {},
  AdvancedLoading: { step: Schema.String, index: Schema.Number },
  StartedGame: {},
  FailedGame: { error: Schema.String, reload: Schema.Boolean },
  GotWorlds: { worlds: Schema.Array(WorldId) },
  ToggledChoice: { choice: Choice, on: Schema.Boolean },
  SucceededFetchCredits: { lines: Schema.Array(CreditLine) },
  FailedFetchCredits: {},
  CompletedSideEffect: {},
});
export type Message = typeof Message.Type;

// FLAGS

export const Flags = Schema.Struct({
  edition: EditionId,
  name: Schema.String,
  world: Schema.String,
  custom: Schema.String,
  telemetry: Schema.Struct({ available: Schema.Boolean, diagnostics: Schema.Boolean, recording: Schema.Boolean }),
  fullscreen: Schema.Boolean,
});
export type Flags = typeof Flags.Type;

const NAME_KEY = "horizon-player-name";

interface Telemetry {
  available?: boolean;
  choices?: () => { diagnostics: boolean; recording: boolean };
  setChoice?: (name: string, on: boolean) => void;
  capture?: (event: string, data?: object) => void;
}
const telemetry = () => (globalThis as { horizonTelemetry?: Telemetry }).horizonTelemetry;

/** Everything the menu starts from: the page's edition, the URL, and what this browser remembers. */
export const readFlags = (edition: EditionId): Effect.Effect<Flags> => Effect.sync(() => {
  const params = new URLSearchParams(location.search);
  let saved = "";
  try { saved = localStorage.getItem(NAME_KEY) ?? ""; } catch {}
  const t = telemetry();
  const choices = t?.choices?.() ?? { diagnostics: false, recording: false };
  return {
    edition,
    name: params.get("name") ?? saved,
    world: params.get("world") ?? (params.get("mode") === "multiplayer" ? "shared" : ""),
    custom: params.get("args") ?? "",
    telemetry: { available: Boolean(t?.available), ...choices },
    fullscreen: Boolean(document.fullscreenElement),
  };
});

const edition = (model: Model): Edition => EDITIONS[model.edition];
const worldOf = (model: Model): World => edition(model).worlds.find((w) => w.id === model.world)!;
const isAvailable = (model: Model, world: WorldId) => world === "solo" || model.available.includes(world);
const nameOk = (name: string) => NAME.test(name);

// COMMANDS

const FetchWorlds = Command.define("FetchWorlds", {
  messages: [Message.GotWorlds],
  execute: Effect.tryPromise(async () => {
    const response = await fetch("./worlds.json", { cache: "no-store" });
    if (!response.ok) throw new Error(String(response.status));
    return await response.json() as { worlds?: unknown };
  }).pipe(
    Effect.map(({ worlds }) => Message.GotWorlds({ worlds: Array.isArray(worlds)
      ? worlds.filter((w): w is WorldId => w === "survival" || w === "shared") : [] })),
    Effect.catch(() => Effect.succeed(Message.GotWorlds({ worlds: [] }))),
  ),
});

const ToggleFullscreen = Command.define("ToggleFullscreen", {
  messages: [Message.CompletedSideEffect],
  execute: Effect.promise(async () => {
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await document.documentElement.requestFullscreen();
    } catch {}
    return Message.CompletedSideEffect();
  }),
});

const SaveChoice = Command.define("SaveChoice", {
  args: { choice: Choice, on: Schema.Boolean },
  messages: [Message.CompletedSideEffect],
  execute: ({ choice, on }) => Effect.sync(() => {
    telemetry()?.setChoice?.(choice, on);
    return Message.CompletedSideEffect();
  }),
});

/** Keeps the name for next time, and the choice in the URL so a reload returns to it. */
const RememberPlayer = Command.define("RememberPlayer", {
  args: { name: Schema.String, world: WorldId },
  messages: [Message.CompletedSideEffect],
  execute: ({ name, world }) => Effect.sync(() => {
    try { localStorage.setItem(NAME_KEY, name); } catch {}
    const url = new URL(location.href);
    url.searchParams.set("name", name);
    url.searchParams.set("world", world);
    url.searchParams.set("mode", world === "solo" ? "singleplayer" : "multiplayer");
    history.replaceState(null, "", url);
    telemetry()?.capture?.("game_session_selected", { selected_mode: world === "solo" ? "singleplayer" : "multiplayer", world });
    return Message.CompletedSideEffect();
  }),
});

const FocusGame = Command.define("FocusGame", {
  messages: [Message.CompletedSideEffect],
  execute: Effect.sync(() => {
    document.getElementById("horizon")?.focus();
    return Message.CompletedSideEffect();
  }),
});

const ReloadPage = Command.define("ReloadPage", {
  messages: [Message.CompletedSideEffect],
  execute: Effect.sync(() => {
    location.reload();
    return Message.CompletedSideEffect();
  }),
});

/** The open edition's credits (its art is CC BY-SA and CC0), as plain lines with links. */
const FetchCredits = Command.define("FetchCredits", {
  messages: [Message.SucceededFetchCredits, Message.FailedFetchCredits],
  execute: Effect.tryPromise(async () => {
    const response = await fetch("./credits.json", { cache: "no-store" });
    if (!response.ok) throw new Error(String(response.status));
    return creditLines(await response.json());
  }).pipe(
    Effect.map((lines) => Message.SucceededFetchCredits({ lines })),
    Effect.catch(() => Effect.succeed(Message.FailedFetchCredits())),
  ),
});

type Credits = {
  artwork_source?: { name?: string; repository?: string; revision?: string };
  artwork?: { author?: string; license?: string; files?: string[] }[];
  fonts?: { name?: string; author?: string; license?: string }[];
  original?: string;
};
export function creditLines(credits: Credits): CreditLine[] {
  const source = credits.artwork_source ?? {};
  const lines: CreditLine[] = [{
    text: `Block artwork from ${source.name ?? "Minetest Game"} (revision ${String(source.revision ?? "").slice(0, 12)})`,
    href: source.repository && source.revision ? `${source.repository}/tree/${source.revision}` : "",
  }];
  for (const entry of credits.artwork ?? []) lines.push({ text: `${entry.author}: ${entry.license}`, href: "" });
  for (const font of credits.fonts ?? []) lines.push({ text: `${font.name} by ${font.author}: ${font.license}`, href: "" });
  if (credits.original) lines.push({ text: credits.original, href: "" });
  lines.push({ text: "Full credits and licences", href: "./credits/CREDITS.md" });
  return lines;
}

// UPDATE

type Return = Update.Return<Model, Message>;
const startLoading = (model: Model): Return => ({
  model: modifyFields(model, { phase: () => Phase.Loading({ step: "Starting", index: 0 }), sheet: () => false }),
  commands: [RememberPlayer({ name: model.name, world: model.world })],
});

export const update = (model: Model, message: Message): Return =>
  Message.match<Return>(message, {
    ChangedName: ({ value }) => ({ model: modifyFields(model, { name: () => value }) }),
    ChoseWorld: ({ world }) => Phase.guards.Loading(model.phase) || !isAvailable(model, world)
      ? { model } : { model: modifyFields(model, { world: () => world }) },
    ToggledSheet: () => {
      const open = !model.sheet;
      const wantCredits = open && edition(model).credits && model.credits === "Idle";
      return {
        model: modifyFields(model, { sheet: () => open, ...(wantCredits && { credits: () => "Loading" as const }) }),
        commands: wantCredits ? [FetchCredits()] : [],
      };
    },
    ClosedSheet: () => ({ model: modifyFields(model, { sheet: () => false }) }),
    ClickedFullscreen: () => ({ model, commands: [ToggleFullscreen()] }),
    ChangedFullscreen: ({ on }) => ({ model: modifyFields(model, { fullscreen: () => on }) }),
    SubmittedPlay: () => {
      if (Phase.guards.Loading(model.phase) || Phase.guards.Playing(model.phase)) return { model };
      if (Phase.guards.Failed(model.phase) && model.phase.reload) return { model, commands: [ReloadPage()] };
      if (!nameOk(model.name) || !isAvailable(model, model.world)) return { model };
      return startLoading(model);
    },
    ChangedPad: ({ pad }) => ({ model: modifyFields(model, { pad: () => pad }) }),
    // ○ / B closes How to play, or cancels a download as Cancel does.
    PressedPadBack: () => model.sheet ? { model: modifyFields(model, { sheet: () => false }) }
      : update(model, Message.ClickedCancel()),
    ClickedCancel: () => Phase.guards.Loading(model.phase) && model.phase.index < STEPS - 1
      ? { model: modifyFields(model, { phase: () => Phase.Menu() }) } : { model },
    AdvancedLoading: ({ step, index }) => Phase.guards.Loading(model.phase)
      ? { model: modifyFields(model, { phase: () => Phase.Loading({ step, index }) }) } : { model },
    StartedGame: () => ({ model: modifyFields(model, { phase: () => Phase.Playing() }), commands: [FocusGame()] }),
    FailedGame: ({ error, reload }) => ({ model: modifyFields(model, { phase: () => Phase.Failed({ error, reload }) }) }),
    GotWorlds: ({ worlds }) => {
      const next = modifyFields(model, { available: () => worlds, checkedWorlds: () => true });
      // A remembered choice this deployment doesn't serve falls back to the first world it does.
      if (isAvailable(next, next.world)) return { model: next };
      const fallback = edition(next).worlds.find((w) => isAvailable(next, w.id))!.id;
      return { model: modifyFields(next, { world: () => fallback }) };
    },
    ToggledChoice: ({ choice, on }) => ({
      model: modifyFields(model, { telemetry: (t) => ({ ...t, [choice]: on }) }),
      commands: [SaveChoice({ choice, on })],
    }),
    SucceededFetchCredits: ({ lines }) => ({ model: modifyFields(model, { credits: () => "Loaded" as const, creditLines: () => lines }) }),
    FailedFetchCredits: () => ({ model: modifyFields(model, { credits: () => "Failed" as const }) }),
    CompletedSideEffect: () => ({ model }),
  });

// INIT

export const init: Runtime.ApplicationInit<Model, Message, Flags> = (flags) => {
  const known = EDITIONS[flags.edition].worlds.find((w) => w.id === flags.world)?.id;
  const model: Model = {
    edition: flags.edition,
    name: flags.name,
    world: known ?? "survival",
    available: [],
    checkedWorlds: false,
    sheet: false,
    fullscreen: flags.fullscreen,
    pad: null,
    phase: Phase.Menu(),
    custom: flags.custom,
    telemetry: flags.telemetry,
    credits: "Idle",
    creditLines: [],
  };
  // Development: ?args= starts the game straight away with those arguments.
  if (flags.custom) return { model: modifyFields(model, { world: () => known ?? "solo",
    phase: () => Phase.Loading({ step: "Starting", index: 0 }) }) };
  return { model, commands: [FetchWorlds()] };
};

// SUBSCRIPTIONS

const toMessage = (progress: Progress): Message => {
  switch (progress._tag) {
    case "Step": return Message.AdvancedLoading({ step: progress.step, index: progress.index });
    case "Started": return Message.StartedGame();
    case "Failed": return Message.FailedGame({ error: progress.error, reload: progress.reload });
  }
};

const padMessage = (event: PadEvent): Message => event._tag === "Changed"
  ? Message.ChangedPad({ pad: event.pad ?? null })
  : event.button === "confirm" ? Message.SubmittedPlay() : Message.PressedPadBack();

export const subscriptions = Subscription.make<Model, Message>()((entry) => ({
  // Keep the subscription alive through play to receive fatal renderer errors.
  game: entry(
    { active: Schema.Boolean, edition: EditionId, world: WorldId, name: Schema.String, custom: Schema.String },
    {
      modelToDependencies: (model) => ({ active: Phase.guards.Loading(model.phase) || Phase.guards.Playing(model.phase), edition: model.edition,
        world: model.world, name: model.name, custom: model.custom }),
      dependenciesToStream: ({ active, edition, world, name, custom }) => active
        ? Stream.fromAsyncIterable(boot(EDITIONS[edition], world, name, custom || undefined), String).pipe(
          Stream.map(toMessage),
          Stream.orElseSucceed((error) => Message.FailedGame({ error, reload: true })))
        : Stream.empty,
    },
  ),
  fullscreen: entry({}, {
    modelToDependencies: () => ({}),
    dependenciesToStream: () => Stream.fromEventListener(document, "fullscreenchange").pipe(
      Stream.map(() => Message.ChangedFullscreen({ on: Boolean(document.fullscreenElement) }))),
  }),
  // The menu's controller: shown while the menu is; the game reads pads itself.
  pad: entry({ menu: Schema.Boolean }, {
    modelToDependencies: (model) => ({ menu: !Phase.guards.Playing(model.phase) }),
    dependenciesToStream: ({ menu }) => menu
      ? Stream.fromAsyncIterable(padEvents(), String).pipe(Stream.map(padMessage), Stream.orElseSucceed(() => Message.CompletedSideEffect()))
      : Stream.empty,
  }),
  escape: entry({ sheet: Schema.Boolean }, {
    modelToDependencies: (model) => ({ sheet: model.sheet }),
    dependenciesToStream: ({ sheet }) => sheet
      ? Stream.fromEventListener<KeyboardEvent>(document, "keydown").pipe(
        Stream.filter((event) => event.key === "Escape"),
        Stream.map(() => Message.ClosedSheet()))
      : Stream.empty,
  }),
}));

// VIEW

const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

export const view = (model: Model, h: HtmlBuilder<Message>): Document => {
  const ed = edition(model);
  return {
    title: Phase.guards.Playing(model.phase) ? ed.name : ed.title,
    body: Phase.guards.Playing(model.phase) ? playing(model, h) : menu(model, h),
  };
};

const icon = (h: HtmlBuilder<Message>, name: string) => h.span([h.Class(`icon icon-${name}`), h.AriaHidden(true)], []);
const kbd = (h: HtmlBuilder<Message>, key: string) => h.kbd([], [key]);

/** The game owns the page; a short hint fades out over it. */
const playing = (model: Model, h: HtmlBuilder<Message>): Html =>
  h.div([h.Class("hint-toast"), h.Role("status")], model.pad ? [
    h.span([], [`${model.pad.name} is ready`]),
    h.span([h.Class("muted")], ["Click the game to use the mouse too"]),
  ] : [
    h.span([], ["Click the game to play"]),
    h.span([h.Class("muted")], [kbd(h, "Esc"), " frees the mouse"]),
  ]);

function menu(model: Model, h: HtmlBuilder<Message>): Html {
  const ed = edition(model), world = worldOf(model);
  const loading = Phase.guards.Loading(model.phase);
  return h.div([h.Class("menu")], [
    h.div([h.Class("backdrop"), h.AriaHidden(true)],
      [...new Set(ed.worlds.map((w) => w.art))].map((art) =>
        h.div([h.Class(`art art-${art}${art === world.art ? " shown" : ""}`)], []))),
    h.header([h.Class("bar")], [
      ...(model.pad ? [h.span([h.Class("chip glass"), h.Role("status")], [icon(h, "gamepad"), model.pad.name])] : []),
      h.button([h.Class("btn glass"), h.Type("button"), h.AriaExpanded(model.sheet), h.AriaControls("how-to-play"),
        h.OnClick(Message.ToggledSheet())], [icon(h, "info"), "How to play"]),
      h.button([h.Class("btn glass"), h.Type("button"), h.OnClick(Message.ClickedFullscreen())],
        [icon(h, "monitor"), model.fullscreen ? "Exit full screen" : "Full screen"]),
    ]),
    h.main([h.Class("content")], [
      h.h1([h.Class("rise")], [ed.name]),
      h.p([h.Class("tagline rise"), h.Style({ "--n": "1" })], [ed.tagline]),
      h.form([h.Class("play rise"), h.Style({ "--n": "2" }), h.OnSubmit(Message.SubmittedPlay()), h.Novalidate(true)], [
        nameField(model, h),
        worldCards(model, h),
        ...(world.caveat ? [h.p([h.Class("caveat")], [world.caveat])] : []),
        ...(!loading ? [h.p([h.Class("touch-controls")], [
          "Connect a keyboard and mouse to play. Use WASD to move and the arrow keys to look around.",
        ])] : []),
        actions(model, h),
        ...(Phase.guards.Failed(model.phase)
          ? [h.p([h.Class("error"), h.Role("alert")], [model.phase.error])] : []),
      ]),
    ]),
    sheet(model, h, loading),
  ]);
}

function nameField(model: Model, h: HtmlBuilder<Message>): Html {
  const bad = model.name !== "" && !nameOk(model.name);
  return h.div([h.Class("who")], [
    h.label([h.For("player-name"), h.Class("label")], [icon(h, "user"), h.span([h.Class("sr")], ["Player name"])]),
    h.input([h.Id("player-name"), h.Class("field"), h.Type("text"), h.Value(model.name), h.Maxlength(16),
      h.Autocomplete("off"), h.Spellcheck(false), h.Placeholder("Your player name"), h.AriaInvalid(bad),
      h.AriaDescribedBy("name-hint"), h.Disabled(Phase.guards.Loading(model.phase)),
      h.OnInput((value) => Message.ChangedName({ value }))]),
    h.p([h.Id("name-hint"), h.Class(`hint${bad ? " bad" : ""}`)],
      [model.name === "" ? "This is how other players see you."
        : bad ? "Use 3–16 letters, numbers or _." : "Looks good."]),
  ]);
}

function worldCards(model: Model, h: HtmlBuilder<Message>): Html {
  const loading = Phase.guards.Loading(model.phase);
  return h.div([h.Class("cards"), h.Role("radiogroup"), h.AriaLabel("World")], edition(model).worlds.map((w) => {
    const available = isAvailable(model, w.id);
    const status = w.id === "solo" ? "On this device" : !model.checkedWorlds ? "Checking…"
      : available ? "Available" : "Not available";
    return h.button([h.Class(`card glass${available ? "" : " off"}`), h.Type("button"), h.Role("radio"),
      h.AriaChecked(w.id === model.world), h.Disabled(!available || loading),
      h.OnClick(Message.ChoseWorld({ world: w.id }))], [
      h.span([h.Class("t")], [w.title]),
      h.span([h.Class("d")], [w.card]),
      h.span([h.Class("n")], [h.span([h.Class(`dot${available && w.id !== "solo" ? "" : " idle"}`)], []), status]),
    ]);
  }));
}

function actions(model: Model, h: HtmlBuilder<Message>): Html {
  const world = worldOf(model);
  if (Phase.guards.Loading(model.phase)) {
    const { step, index } = model.phase;
    return h.div([h.Class("go")], [
      h.div([h.Class("progress"), h.Role("progressbar"), h.AriaValuemin(0), h.AriaValuemax(STEPS),
        h.AriaValuenow(index + 1), h.AriaValuetext(step), h.Style({ "--p": String((index + 1) / STEPS) })], [
        h.span([h.Class("fill")], []),
        h.span([h.Class("what")], [step]),
        ...(index < STEPS - 1 ? [h.button([h.Class("btn cancel"), h.Type("button"), h.OnClick(Message.ClickedCancel())], ["Cancel"])] : []),
      ]),
    ]);
  }
  if (Phase.guards.Failed(model.phase) && model.phase.reload) {
    return h.div([h.Class("go")], [h.button([h.Class("btn primary big"), h.Type("submit")], ["Reload page"])]);
  }
  const ready = nameOk(model.name) && isAvailable(model, model.world);
  return h.div([h.Class("go")], [
    h.button([h.Id("play"), h.Class("btn primary big"), h.Type("submit"), h.Disabled(!ready)],
      [icon(h, "play"), world.id === "solo" ? "Play single player" : `Join ${world.title}`]),
    h.span([h.Class("note")], [Phase.guards.Failed(model.phase) ? "Press again to retry."
      : model.pad && ready ? `Or press ${model.pad.confirm} on your controller.`
      : "The game downloads when you press Play, then stays cached."]),
  ]);
}

function sheet(model: Model, h: HtmlBuilder<Message>, loading: boolean): Html {
  const ed = edition(model);
  const keys: [string, string][] = [["W A S D", "Move"], ["Space", "Jump; twice to fly in creative"],
    ["Left Shift", "Crouch"], ["Left Ctrl", "Run"], ["Left click", "Break"], ["Right click", "Place or use"],
    ["1–9, wheel", "Choose a hotbar slot"], ["E", "Inventory"], ["T", "Chat"], ["F5", "Change view"],
    ["Esc", "Pause and free the mouse"]];
  return h.aside([h.Id("how-to-play"), h.Class(`sheet${model.sheet ? " open" : ""}`), h.AriaLabel("How to play"),
    h.AriaHidden(!model.sheet), h.Inert(!model.sheet)], [
    h.button([h.Class("btn close"), h.Type("button"), h.AriaLabel("Close"), h.OnClick(Message.ClosedSheet())], ["✕"]),
    h.h2([], ["How to play"]),
    h.p([h.Class("muted")], ["Everything you need before you load in."]),
    h.section([], [
      h.h3([], [icon(h, "monitor"), "Full screen"]),
      h.p([], [...(isMac ? [kbd(h, "⌃"), " ", kbd(h, "⌘"), " ", kbd(h, "F")] : [kbd(h, "F11")]),
        " or the Full screen button. Click inside the game to capture the mouse; ", kbd(h, "Esc"), " frees it again."]),
    ]),
    h.section([], [
      h.h3([], [icon(h, "info"), "Keyboard and mouse"]),
      h.dl([h.Class("keys")], keys.flatMap(([key, action]) => [h.dt([], [kbd(h, key)]), h.dd([], [action])])),
    ]),
    controller(model, h),
    h.section([], [
      h.h3([], [icon(h, "globe"), "The worlds"]),
      ...ed.worlds.map((w) => h.p([], [h.strong([], [`${w.title}. `]), w.about])),
    ]),
    ...(model.telemetry.available ? [h.section([], [
      h.h3([], [icon(h, "bug"), "Diagnostics"]),
      h.p([], ["Optional. Helps fix problems; recordings may include on-screen gameplay and chat."]),
      choice(h, "diagnostics", "Share diagnostics", model.telemetry.diagnostics, loading),
      choice(h, "recording", "Record sessions", model.telemetry.recording, loading),
    ])] : []),
    ...(ed.credits ? [h.section([], [
      h.h3([], [icon(h, "heart"), "Credits"]),
      ...(model.credits === "Loaded" ? model.creditLines.map((line) => h.p([h.Class("credit")],
        [line.href ? h.a([h.Href(line.href), h.Target("_blank"), h.Rel("noopener")], [line.text]) : line.text]))
        : [h.p([h.Class("muted")], [model.credits === "Failed" ? "The credits couldn't load." : "Loading…"])]),
    ])] : []),
  ]);
}

/** Connecting a controller: browsers show one to the page only after a button press. */
function controller(model: Model, h: HtmlBuilder<Message>): Html {
  const pad = model.pad;
  return h.section([], [
    h.h3([], [icon(h, "gamepad"), "Controller"]),
    h.p([h.Class("pad-status"), h.Role("status")], pad
      ? [h.span([h.Class("dot")], []), h.span([], [h.strong([], [pad.name]), ` is connected. ${pad.confirm} plays, ${pad.back} goes back.`])]
      : [h.span([h.Class("dot idle")], []), h.span([], ["None yet. Connect one, then press any button on it."])]),
    h.p([], [h.strong([], ["USB. "]), "Plug it in."]),
    h.p([], [h.strong([], ["Bluetooth. "]), "On a PS5 controller, hold PS and Create until the light blinks; on a PS4 controller, PS and Share; on an Xbox controller, the pair button on top. Then pair it in your computer's Bluetooth settings."]),
    h.p([], ["The layout is Bedrock's: left stick moves, right stick looks, ", kbd(h, pad?.confirm ?? "A"),
      " jumps, the right trigger breaks, the left trigger uses. Change it in Options › Controls › Controller Settings."]),
  ]);
}

const choice = (h: HtmlBuilder<Message>, name: "diagnostics" | "recording", label: string, on: boolean, disabled: boolean) =>
  h.label([h.Class("check ph-no-capture")], [
    h.input([h.Type("checkbox"), h.Checked(on), h.Disabled(disabled),
      h.OnChange(() => Message.ToggledChoice({ choice: name, on: !on }))]),
    label,
  ]);
