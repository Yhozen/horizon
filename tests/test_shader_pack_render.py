"""Unit tests for the pure parts of tools/shader_pack_render.py (synthetic pictures only)."""

import sys
import unittest
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))

import shader_pack_render as render  # noqa: E402


def picture(w, h, f):
    return [f(x, y) for y in range(h) for x in range(w)]


class Png(unittest.TestCase):
    def test_round_trip_and_filters(self):
        px = picture(7, 5, lambda x, y: (x * 30, y * 50, (x * y) % 256))
        w, h, back = render.decode_png(render.encode_png(7, 5, px))
        self.assertEqual((w, h, back), (7, 5, px))
        # Every filter type, RGBA input: rebuild the PNG with filters 1..4 by hand.
        rgba = [(r, g, b, 200) for r, g, b in px]
        raw = bytearray()
        prev = bytes(7 * 4)
        for y in range(5):
            line = bytes(v for p in rgba[y * 7:(y + 1) * 7] for v in p)
            f = y % 5
            out = bytearray()
            for i, v in enumerate(line):
                a = line[i - 4] if i >= 4 else 0
                b = prev[i]
                c = prev[i - 4] if i >= 4 else 0
                pred = [0, a, b, (a + b) // 2, None][f]
                if f == 4:
                    p = a + b - c
                    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                    pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                out.append((v - pred) & 0xFF)
            raw += bytes([f]) + out
            prev = line
        png = render.encode_png(1, 1, [(0, 0, 0)])
        ihdr = b"\x00\x00\x00\x07\x00\x00\x00\x05\x08\x06\x00\x00\x00"
        def chunk(kind, body):
            return len(body).to_bytes(4, "big") + kind + body + zlib.crc32(kind + body).to_bytes(4, "big")
        png = png[:8] + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(bytes(raw))) + chunk(b"IEND", b"")
        self.assertEqual(render.decode_png(png)[2], px)


class Health(unittest.TestCase):
    def test_black_uniform_and_a_scene(self):
        black = render.health(32, 18, picture(32, 18, lambda x, y: (0, 0, 0)))
        self.assertIn("black", black["problems"])
        grey = render.health(32, 18, picture(32, 18, lambda x, y: (90, 90, 90)))
        self.assertIn("uniform", grey["problems"])
        self.assertIn("sky = ground", grey["problems"])
        scene = picture(32, 18, lambda x, y: (120, 170, 255) if y < 8 else (60 + x, 120, 40))
        self.assertEqual(render.health(32, 18, scene)["problems"], [])
        magenta = picture(32, 18, lambda x, y: (255, 0, 255) if x < 8 else (120, 170, 255) if y < 8 else (60, 120, 40))
        self.assertTrue(any(p.startswith("magenta") for p in render.health(32, 18, magenta)["problems"]))

    def test_similarity_masks_the_hud(self):
        a = picture(32, 18, lambda x, y: (100, 100, 100))
        b = picture(32, 18, lambda x, y: (255, 255, 255) if y >= 14 else (100, 100, 100))
        ga, gb = render.grid(32, 18, a), render.grid(32, 18, b)
        self.assertEqual(render.similarity(ga, gb), 0.0)
        c = picture(32, 18, lambda x, y: (110, 100, 100))
        self.assertAlmostEqual(render.similarity(ga, render.grid(32, 18, c)), 10 / 3, places=3)

    def test_verdicts(self):
        base = {"shot": True, "code": 0, "health": {"problems": []}}
        self.assertEqual(render.verdict(base), "Draws")
        self.assertEqual(render.verdict({**base, "vanilla_diff": 1.0}), "Looks vanilla")
        self.assertEqual(render.verdict({**base, "health": {"problems": ["black"]}}), "Black")
        self.assertEqual(render.verdict({"shot": False, "code": 101}), "Crash")
        self.assertEqual(render.verdict({"shot": False, "code": -1}), "Timeout")


if __name__ == "__main__":
    unittest.main()
