import posthog from "posthog-js";

const config = __HORIZON_POSTHOG__;
const key = "horizon-diagnostics-v1";
const controls = { diagnostics: document.getElementById("diagnostics"), recording: document.getElementById("recording") };
let choices = { diagnostics: false, recording: false }, initialized = false;
let errorWindow = 0, errorCount = 0;
try { choices = { ...choices, ...JSON.parse(localStorage.getItem(key) ?? "{}") }; } catch {}
const cleanText = (value) => String(value).slice(0, 2000)
  .replace(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/gi, "[email]")
  .replace(/\b(?:\d{1,3}\.){3}\d{1,3}\b/g, "[address]")
  .replace(/(https?:\/\/[^\s?#]+)[?#][^\s]*/g, "$1")
  .replace(/(?:phc_|phx_)[\w-]+/g, "[token]");
function safeProperties(value) {
  if (typeof value === "string") return cleanText(value);
  if (Array.isArray(value)) return value.map(safeProperties);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(Object.entries(value).filter(([name]) =>
    !/^(cookie|authorization|chat|players?|player_positions|address|name|args|\$set|\$set_once)$/i.test(name))
    .map(([name, item]) => [name, safeProperties(item)]));
}
const properties = () => ({ app: config.app ?? "horizon-web", telemetry_schema: 1, git_commit: config.commit,
  stage: config.stage, release_id: window.horizonRelease ?? "loading", mode: new URLSearchParams(location.search).get("mode") === "multiplayer" ? "multiplayer" : "singleplayer" });
const guarded = (action) => { try { return action(); } catch (error) {
  if (location.hostname === "127.0.0.1") console.warn("PostHog integration:", error);
} };
function capture(event, data = {}) {
  if (choices.diagnostics && initialized) guarded(() => posthog.capture(event, { ...properties(), ...safeProperties(data) }));
}
function logInfo(message, data = {}) {
  if (choices.diagnostics && initialized) guarded(() => posthog.logger.info(message, { ...properties(), ...safeProperties(data) }));
}
function captureException(error, data = {}) {
  if (choices.diagnostics && initialized) guarded(() => posthog.captureException(error instanceof Error ? error : new Error(cleanText(error)), { ...properties(), ...safeProperties(data) }));
}
function apply() {
  if (!config.token || !config.host) return;
  guarded(() => {
    if (!choices.diagnostics && !choices.recording) {
      if (initialized) { posthog.stopSessionRecording(); posthog.stopExceptionAutocapture(); posthog.opt_out_capturing(); }
      return;
    }
    if (!initialized) {
      posthog.init(config.token, {
        api_host: config.host, defaults: "2026-08-30", persistence: "localStorage",
        opt_out_useragent_filter: location.hostname === "127.0.0.1",
        autocapture: false, capture_pageview: false, capture_pageleave: false,
        person_profiles: "never", disable_session_recording: !choices.recording,
        capture_exceptions: false, enable_recording_console_log: false,
        logs: { captureConsoleLogs: false, serviceName: config.app ?? "horizon-web", environment: config.stage, serviceVersion: config.commit },
        capture_performance: false, disable_surveys: true,
        session_recording: { maskAllInputs: true, recordBody: false, recordHeaders: false,
          maskCapturedNetworkRequestFn: () => null },
        before_send: (event) => {
          if (!event || (!choices.diagnostics && event.event !== "$snapshot")) return null;
          if (event.event === "$exception") {
            if (Date.now() - errorWindow >= 60_000) { errorWindow = Date.now(); errorCount = 0; }
            if (++errorCount > 20) return null;
          }
          // Replay snapshots use their own masking settings; never rewrite rrweb's structure.
          if (event.event !== "$snapshot") {
            event.properties = safeProperties(event.properties);
            // The SDK's public routing token is required by ingestion, not user data.
            event.properties.token = config.token;
          }
          return event;
        },
        loaded: () => { window.dispatchEvent(new Event("horizon:telemetry-ready")); },
      });
      initialized = true;
    }
    posthog.opt_in_capturing({ captureEventName: false });
    posthog.register(properties());
    if (choices.diagnostics) posthog.startExceptionAutocapture({ capture_unhandled_errors: true, capture_unhandled_rejections: true, capture_console_errors: true });
    else posthog.stopExceptionAutocapture();
    if (choices.recording) posthog.startSessionRecording(); else posthog.stopSessionRecording();
    capture("diagnostics_enabled", { recording: choices.recording });
    logInfo("diagnostics enabled", { recording_enabled: choices.recording });
  });
}
for (const [name, input] of Object.entries(controls)) {
  if (!input) continue;
  input.checked = Boolean(choices[name]);
  input.disabled = !config.token;
  input.addEventListener("change", () => {
    choices[name] = input.checked;
    try { localStorage.setItem(key, JSON.stringify(choices)); } catch {}
    apply();
  });
}
// The page's menu reads and changes the two choices through these.
function setChoice(name, on) {
  if (!(name in choices) || !config.token) return;
  choices[name] = Boolean(on);
  try { localStorage.setItem(key, JSON.stringify(choices)); } catch {}
  apply();
}
window.horizonTelemetry = { capture, captureException, logInfo, setRelease: () => guarded(() => initialized && posthog.register(properties())),
  available: Boolean(config.token), choices: () => ({ ...choices }), setChoice,
  session: () => initialized ? { id: posthog.get_session_id(), recording: posthog.sessionRecordingStarted(), capturing: !posthog.has_opted_out_capturing(), url: posthog.get_session_replay_url() } : null };
document.getElementById("session")?.addEventListener("submit", (event) => capture("game_session_selected", { selected_mode: event.submitter?.value }));

let previous, screen, joined = false, loaded = false;
setInterval(() => {
  if (!choices.diagnostics || document.visibilityState !== "visible") { previous = undefined; return; }
  const game = window.horizonGame;
  if (!game) return;
  if (game.screen !== screen) { capture("game_screen_changed", { screen: game.screen }); screen = game.screen; }
  if (game.remote?.joined && !joined) capture("multiplayer_joined", { elapsed_ms: Math.round(performance.now()) });
  joined = Boolean(game.remote?.joined);
  if (!loaded && game.screen === "Playing" && game.chunks > 0 && game.meshed > 0) {
    const elapsed_ms = Math.round(performance.now()), multiplayer = Boolean(game.remote);
    capture("game_playable", { elapsed_ms, multiplayer });
    logInfo("game became playable", { elapsed_ms, multiplayer });
    loaded = true;
  }
  const now = performance.now();
  if (!previous) previous = { time: now, frame: game.frame };
  if (now - previous.time >= 30_000) {
    capture("game_performance", { seconds: (now - previous.time) / 1000,
      fps: Math.max(0, (game.frame - previous.frame) * 1000 / (now - previous.time)),
      screen: game.screen, chunks: game.chunks, meshed: game.meshed, decode_errors: game.remote?.decode_errors ?? 0 });
    previous = { time: now, frame: game.frame };
  }
}, 2000);
apply();
