"""Exercise the real item-level cache across LAN guest respawn swaps.

Only world/sky data is stubbed, so no window or prepared Minecraft resources are
needed. enter_guest/leave_guest swap Game.spawn without advancing the clocks.
"""
from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


def method(source, name):
    start = source.rfind("\n", 0, source.index(f"fn {name}(")) + 1
    opening = source.index("{", start)
    depth, end = 1, opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end].replace("pub(super) ", "")


class ItemLevelCacheTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("rustc"), "requires Rust")
    def test_guest_respawn_does_not_leak_into_host_render(self):
        source = (ROOT / "crates/horizon-client/src/game/item_draw.rs").read_text(encoding="utf-8")
        fixture = r'''
use std::cell::{Cell, RefCell};
#[allow(dead_code)]
enum Dim { Overworld, Nether, End }
#[derive(Clone, Copy)]
struct Pos { x: f64, y: f64, z: f64 }
#[derive(Clone, Debug, PartialEq)]
struct GlobalPos { dimension: String, pos: [i32; 3] }
#[derive(Clone, Debug, PartialEq)]
struct LevelState {
    dimension: String, game_time: i64, sun_angle: f32, moon_phase: i32,
    respawn: Option<GlobalPos>, runs_normally: bool,
}
struct World { game_time: i64, day_time: i64 }
struct Sky { sun_angle: f32 }
struct Environment { samples: Cell<u32> }
impl Environment {
    fn sample(&self, _: i64, _: Option<()>, _: Option<()>) -> Sky {
        self.samples.set(self.samples.get() + 1);
        Sky { sun_angle: 0.0 }
    }
}
struct Game {
    dim: Dim, frames: u64, world: World, spawn: Option<Pos>, environment: Environment,
    item_level_cache: RefCell<Option<((u64, i64, i64, &'static str), LevelState)>>,
}
impl Game {
    fn dim_environment(&self) -> &Environment { &self.environment }
METHODS
}
fn main() {
    let host = Some(Pos { x: 100.5, y: 64.0, z: 0.5 });
    let mut game = Game {
        dim: Dim::Overworld, frames: 42, world: World { game_time: 100, day_time: 100 },
        spawn: host, environment: Environment { samples: Cell::new(0) },
        item_level_cache: RefCell::new(None),
    };
    // The guest's eating particles resolve an item before the host renders.
    let guest = Some(Pos { x: -100.5, y: 64.0, z: 0.5 });
    for spawn in [guest, host, None, host] {
        game.spawn = spawn;
        let expected = game.level_state_now("minecraft:overworld");
        assert_eq!(game.item_level(), expected, "respawn must follow the acting player");
        let samples = game.environment.samples.get();
        for _ in 0..45 {
            assert_eq!(game.item_level(), expected);
        }
        assert_eq!(game.environment.samples.get(), samples, "unchanged items reuse the sky sample");
    }
}
'''
        fixture = fixture.replace("METHODS", method(source, "item_level") + "\n" +
                                  method(source, "level_state_now"))
        with tempfile.TemporaryDirectory(prefix="horizon-item-level-") as folder:
            folder = Path(folder)
            rust = folder / "item_level.rs"
            binary = folder / ("item_level.exe" if os.name == "nt" else "item_level")
            rust.write_text(fixture, encoding="utf-8")
            subprocess.run(["rustc", "--edition=2024", str(rust), "-o", str(binary)],
                           check=True, capture_output=True, text=True, timeout=60)
            result = subprocess.run([str(binary)], capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
