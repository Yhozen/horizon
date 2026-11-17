#!/bin/bash
# Records Horizon Open Creative's browser proof (docs/open-creative.md): three
# videos of the static open build driven with real keyboard and mouse input.
#
#   tools/web/open_proof.sh OUT_DIR [URL]
#
# Needs Xvfb, ffmpeg, agent-browser and node. The headed browser runs on its own
# Xvfb display, which ffmpeg records (x11grab): agent-browser's own screencast
# does not receive frames from a WebGPU canvas. agent-browser drives the page;
# tools/web/cdp_input.mjs sends held keys and mouse motion through the same
# browser's DevTools input domain. Positions come from the game's progress lines.
set -u
OUT=$(realpath -m "${1:?usage: open_proof.sh OUT_DIR [URL]}")
URL=${2:-http://127.0.0.1:8081/}
HERE=$(cd "$(dirname "$0")" && pwd)
DISPLAY_NUMBER=${OPEN_PROOF_DISPLAY:-:88}
mkdir -p "$OUT"
export DISPLAY=$DISPLAY_NUMBER AGENT_BROWSER_SESSION=open-proof
LOG="$OUT/proof.log"
: > "$LOG"

if ! pgrep -f "Xvfb $DISPLAY_NUMBER " >/dev/null; then
  Xvfb "$DISPLAY_NUMBER" -screen 0 1440x1000x24 -nolisten tcp >/dev/null 2>&1 &
  sleep 1
fi

ab() { agent-browser "$@" 2>&1 | tail -1; }
note() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG"; }
input() { node "$HERE/cdp_input.mjs" "$(agent-browser get cdp-url 2>/dev/null | tail -1)" "$@"; }
pos() { agent-browser console 2>/dev/null | grep "Horizon web: frame" | tail -1 | sed 's/.*player Some(\[\(.*\)\]).*/\1/'; }
shot() {
  agent-browser screenshot >/dev/null 2>&1
  cp "$HOME/.agent-browser/tmp/screenshots/$(ls -t "$HOME/.agent-browser/tmp/screenshots/" | head -1)" "$OUT/$1.png"
  note "screenshot $1.png"
}
# Page pixels of a GUI point (the canvas sits at 20,61 and the GUI scale is 3).
gui() { echo $((20 + $1 * 3)) $((61 + $2 * 3)); }
click_gui() {
  read -r x y <<<"$(gui "$1" "$2")"
  input move:$x:$y wait:400 click:left
}
wait_playing() {
  for _ in $(seq 1 60); do
    s=$(agent-browser eval "window.horizonStatus" 2>/dev/null | tail -1)
    case "$s" in *Playing*) note "page: $s"; return 0 ;; *Failed*) note "page: $s"; return 1 ;; esac
    sleep 2
  done
  note "page did not start"; return 1
}
locked() { agent-browser eval 'String(document.pointerLockElement && document.pointerLockElement.id)' 2>/dev/null | tail -1; }
capture() {
  # Take the mouse: click the canvas centre until the pointer is locked.
  for _ in 1 2 3 4; do
    [ "$(locked)" = '"horizon"' ] && break
    input move:660:420 wait:200 click:left wait:1800
    [ "$(locked)" = '"horizon"' ] && break
  done
  note "pointer lock: $(locked)"
}
y_of() { echo "$1" | cut -d, -f2 | cut -d. -f1 | tr -d ' '; }
fly() {
  # Double-tap jump. At software-rendering frame rates the release and the second
  # press must each land on its own frame inside vanilla's 7-tick window, so try
  # a few press/gap timings until the player rises.
  for timing in 150:150 120:170 170:120 140:140 110:190 160:110; do
    press=${timing%:*}; gap=${timing#*:}
    before=$(y_of "$(pos)")
    input hold:Space:$press wait:$gap hold:Space:2600 wait:2300
    after=$(y_of "$(pos)")
    if [ -n "$before" ] && [ -n "$after" ] && [ "$after" -gt $((before + 3)) ]; then
      note "flying (press ${press} ms, gap ${gap} ms): rose from y=$before to y=$after"; return 0
    fi
    sleep 1.5
  done
  note "flight did not start"; return 1
}
record_start() {
  ffmpeg -v error -y -f x11grab -video_size 1440x1000 -framerate 12 -i "$DISPLAY_NUMBER" \
    -c:v libx264 -preset ultrafast -crf 28 -pix_fmt yuv420p "$OUT/$1.mp4" &
  FFMPEG=$!
  note "recording $1.mp4"
}
record_stop() { kill -INT "$FFMPEG"; wait "$FFMPEG" 2>/dev/null; note "recording stopped"; }
# fresh_page [SCALE]: a new browser for the page at PAGE; a scale below 1 renders fewer pixels
# (software rendering is slow; vanilla's double-tap flight needs ~7 frames/s).
fresh_page() {
  agent-browser close >/dev/null 2>&1
  rm -f "${TMPDIR:-/tmp}/cdp-input-mouse.json"
  PAGE="$URL"
  [ "${1:-1}" != 1 ] && PAGE="$URL?scale=$1"
  ab --webgpu --headed --args "--test-type,--no-first-run" open about:blank >/dev/null
  # The window fills the display before the game's page loads (GUI scale 3).
  input window:1440:1000
  sleep 1
}
canvas_offset() {
  agent-browser eval "(() => { const r = document.getElementById('horizon').getBoundingClientRect(); return Math.round(r.left) + ',' + Math.round(r.top + outerHeight - innerHeight); })()" 2>/dev/null | tail -1 | tr -d '"'
}

# A software Vulkan GPU process sometimes starts without an adapter: a recording
# whose page did not start is made again from a fresh browser.
attempt() {
  local name=$1
  for try in 1 2 3; do
    if "$name"; then return 0; fi
    record_stop
    note "$name: the page did not start (try $try); again"
  done
  return 1
}

# 1. Startup and exploration.
rec1() {
fresh_page 1
record_start 1-startup-exploration
ab open "$PAGE" >/dev/null
sleep 4
shot 1-notice
note "notice text: $(agent-browser get text '#notice' | tr '\n' ' ' | head -c 300)"
note "canvas on screen at $(canvas_offset)"
ab click "#enter" >/dev/null
wait_playing || return 1
sleep 6
capture
note "spawn at $(pos)"
input look:500:0 wait:1500 look:-900:60 wait:1500
input hold:KeyW:4000 wait:2200
note "after walking: $(pos)"
input down:KeyW hold:Space:350 wait:900 hold:Space:350 wait:900 hold:Space:350 wait:900 up:KeyW wait:2200
note "after walking and jumping: $(pos)"
fly
shot 1-flying
input look:0:150 hold:KeyW:6000 look:500:0 hold:KeyW:4000 wait:2200
note "after flying over the terrain: $(pos)"
input hold:ShiftLeft:6000 wait:2200
note "after descending: $(pos)"
input look:-600:-100 wait:2000
shot 1-terrain
record_stop
}

# 2. Building: hotbar materials, placing, log axes, breaking, collision.
rec2() {
fresh_page 1
record_start 2-building
ab open "$PAGE" >/dev/null
sleep 2
ab click "#enter" >/dev/null
wait_playing || return 1
sleep 6
capture
note "spawn at $(pos)"
# Go to a flat patch of the (deterministic) terrain with /tp, one of the open
# edition's commands, typed into the chat.
input tap:KeyT wait:700 "type:/tp 10 77 20" wait:300 tap:Enter wait:2500
capture
note "after /tp: $(pos)"
# About 0.15 degrees per mouse pixel: 260 px down looks 39 degrees down, at the
# ground two blocks ahead; 120 px turns 18 degrees.
input look:0:260 wait:800 tap:Digit6 wait:300
for _ in 1 2 3 4; do input click:right wait:800 look:120:0 wait:500; done
input look:-300:-100 wait:1200
shot 2-row
# Logs: on the first brick's near face (lying) and on the second's top (upright).
input look:-180:100 wait:500 tap:Digit5 wait:300 click:right wait:900
input look:120:-115 wait:500 click:right wait:900
# Glass and the light block on the next two bricks.
input tap:Digit7 wait:300 look:120:0 wait:500 click:right wait:900
input tap:Digit8 wait:300 look:120:0 wait:500 click:right wait:900
input look:-180:-30 wait:1500
shot 2-built
# Break the glass again.
input look:60:30 wait:500 click:left wait:1200 look:-60:-30 wait:1200
shot 2-broken
# Step back for an overview, then walk into the build for three seconds.
input look:0:-60 wait:500 hold:KeyS:1300 wait:1500
shot 2-overview
input look:0:-65 wait:2300
note "before walking into the build: $(pos)"
input hold:KeyW:3000 wait:2300
note "after walking into the build for 3 s: $(pos)"
shot 2-collision
record_stop
}

# 3. Visual coverage, credits and the session reset.
rec3() {
fresh_page 1
record_start 3-visuals-credits-reset
ab open "$PAGE" >/dev/null
sleep 2
ab click "#enter" >/dev/null
wait_playing || return 1
sleep 6
capture
note "spawn at $(pos)"
# The block palette: each category, then pick obsidian into the hotbar.
input tap:KeyE wait:1500
shot 3-palette-ground
click_gui 156 40; sleep 1.5; shot 3-palette-construction
click_gui 183 40; sleep 1.5; shot 3-palette-colored
click_gui 129 40; sleep 1
click_gui 277 78; sleep 0.6; click_gui 133 172; sleep 0.8
shot 3-palette-picked
# E closes the palette (in a browser, Escape also ends the pointer lock: pause).
input tap:KeyE wait:1200
capture
# Leaves, glass and water.
input look:300:-80 wait:1500
shot 3-trees
input tap:Digit7 wait:300 look:-300:240 click:right wait:900 look:0:-160 wait:1000
shot 3-glass
# Night: light the block palette's light block.
input tap:KeyT wait:700 "type:/time set 14000" wait:300 tap:Enter wait:2500
capture
input tap:Digit8 wait:300 look:200:200 click:right wait:900 look:0:-120 wait:2500
shot 3-night-light
# Swim: towards the water and into it.
input look:700:0 hold:KeyW:6000 wait:2200
note "towards the water: $(pos)"
input down:KeyW hold:Space:3000 up:KeyW wait:2200
note "swimming: $(pos)"
shot 3-water
# In-game credits from the pause menu.
input tap:Escape wait:1200
shot 3-pause
click_gui 266 102
# The rolling credits start below the screen and scroll up at vanilla's pace.
sleep 16
shot 3-credits-ingame
input tap:Escape wait:1500
# The page's credits dialog.
input tap:Escape wait:800
ab click "#credits-button" >/dev/null
sleep 2
shot 3-credits-page
ab press Escape >/dev/null
sleep 1
# Reload: the session notice again, then a fresh world.
ab reload >/dev/null
sleep 4
shot 3-after-reload-notice
note "after reload notice visible: $(agent-browser eval '!document.getElementById("notice").hidden' | tail -1)"
ab click "#enter" >/dev/null
wait_playing
sleep 6
note "fresh world spawn at $(pos)"
shot 3-fresh-world
record_stop
}

for recording in ${OPEN_PROOF_ONLY:-rec1 rec2 rec3}; do attempt "$recording"; done
agent-browser close >/dev/null 2>&1
note "done"
