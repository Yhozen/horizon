"""Unit tests for the pure parts of tools/shader_pack_corpus.py (synthetic results only)."""

import importlib.util
import sys
import unittest
from pathlib import Path


# Loaded by path: tools/ holds harness scripts named like tests/ modules, so it stays off sys.path.
_spec = importlib.util.spec_from_file_location(
    "shader_pack_corpus", Path(__file__).resolve().parent.parent / "tools" / "shader_pack_corpus.py"
)
corpus = importlib.util.module_from_spec(_spec)
sys.modules["shader_pack_corpus"] = corpus
_spec.loader.exec_module(corpus)

DIM = "minecraft:overworld"


def program(slot, status="ok", error=None, essential=False, spirv=None, chrome=None):
    p = {"slot": slot, "status": status, "essential": essential, "source": f"shaders/{slot}"}
    if error is not None:
        p["error"] = error
    if status == "ok":
        p["spirv_errors"] = spirv or {"vs": None, "fs": None}
        if chrome is not None:
            p["chrome_errors"] = chrome
    return p


def result(name, programs, features=None, plan_error=None):
    return {"pack": name, "features": features or {},
            "dimensions": {DIM: {"programs": programs, "plan_error": plan_error}}}


class NormaliseTest(unittest.TestCase):
    def test_strips_quotes_numbers_and_positions(self):
        a = corpus.normalise("error: Unknown function 'foo' at line 12:4")
        b = corpus.normalise("error: Unknown function 'bar' at line 3:1")
        self.assertEqual(a, b)
        self.assertNotIn("foo", a)

    def test_keeps_only_the_headline(self):
        message = "error: bad thing\n  ┌─ wgsl:3:5\n  │\n3 │ let x = y;\n"
        self.assertEqual(corpus.normalise(message), "bad thing")

    def test_trailing_identifier(self):
        self.assertEqual(corpus.normalise("Unknown variable: hz_light"), corpus.normalise("Unknown variable: abc"))

    def test_keeps_names_that_say_what_is_missing(self):
        self.assertEqual(corpus.normalise("sampler `colortex0` passed to `textureOffset`"),
                         "sampler '…' passed to 'textureOffset'")
        self.assertEqual(corpus.normalise("Unknown function 'hz_sampleLod_shadowtex0'"),
                         "Unknown function 'hz_sampleLod_…'")
        self.assertEqual(corpus.normalise("error: Unknown variable: MC_RENDER_STAGE_SUN"),
                         "Unknown variable: MC_RENDER_STAGE_*")
        self.assertEqual(corpus.normalise("uniform frameCounter is float in the pack"),
                         "uniform … is float in the pack")

    def test_validation_note(self):
        message = "error: Function [7] 'main' is invalid\n  ┌─ wgsl:1:2\n  ·\n  = Expression [3] is bad\n"
        self.assertEqual(corpus.normalise(message), "Function [N] '…' is invalid: Expression [N] is bad")

    def test_paths(self):
        self.assertEqual(corpus.normalise("shaders/lib/a.glsl: missing include"),
                         corpus.normalise("shaders/world0/b.fsh: missing include"))

    def test_naga_detection(self):
        self.assertTrue(corpus.is_naga_error("error: x\n  ┌─ glsl:1:1"))
        self.assertFalse(corpus.is_naga_error("unsupported sampler type sampler3D"))


class StatusTest(unittest.TestCase):
    def test_works_candidate(self):
        row = corpus.pack_row(result("a", [program("gbuffers_terrain", essential=True)]), DIM, False)
        self.assertEqual(row["status"], "Works-candidate")
        self.assertEqual((row["found"], row["translated"], row["naga_ok"], row["spirv_ok"]), (1, 1, 1, 1))

    def test_partial_when_optional_fails(self):
        programs = [program("gbuffers_terrain", essential=True),
                    program("gbuffers_weather", "error", "error: bad\n ┌─ x")]
        row = corpus.pack_row(result("a", programs), DIM, False)
        self.assertEqual(row["status"], "Partial")
        self.assertEqual(row["translated"], 2)
        self.assertEqual(row["naga_ok"], 1)

    def test_fails_when_essential_fails_in_chrome(self):
        programs = [program("final", essential=True, chrome={"vs": [], "fs": ["unresolved value 'x'"]})]
        row = corpus.pack_row(result("a", programs), DIM, True)
        self.assertEqual(row["status"], "Fails")
        self.assertEqual(row["chrome_ok"], 0)
        self.assertEqual(row["essential_failed"], ["final"])

    def test_refused_on_required_features(self):
        r = result("a", [program("final", essential=True)], {"iris_features_required": ["COMPUTE_SHADERS"]})
        self.assertEqual(corpus.pack_row(r, DIM, False)["status"], "Refused")

    def test_disabled_programs_do_not_count(self):
        programs = [program("final", essential=True), program("composite3", "disabled")]
        self.assertEqual(corpus.pack_row(result("a", programs), DIM, False)["found"], 1)

    def test_fails_without_programs(self):
        self.assertEqual(corpus.pack_row(result("a", []), DIM, False)["status"], "Fails")

    def test_not_a_pack_without_program_sources(self):
        r = {**result("a", []), "shader_sources": 0}
        self.assertEqual(corpus.pack_row(r, DIM, False)["status"], "Not a pack")


class BucketTest(unittest.TestCase):
    def test_groups_and_sorts_by_packs_blocked(self):
        results = [
            result("a", [program("gbuffers_water", "error", "Unknown function 'f'", essential=True),
                         program("gbuffers_weather", "error", "Unknown function 'g'")]),
            result("b", [program("gbuffers_terrain", "error", "Unknown function 'h'", essential=True)]),
            result("c", [program("gbuffers_clouds", "error", "other problem 3")]),
        ]
        groups = corpus.buckets(results, [DIM])
        self.assertEqual(len(groups), 2)
        first = groups[0]
        self.assertEqual(first["count"], 3)
        self.assertEqual(first["packs"], ["a", "b"])
        self.assertEqual(first["essential_packs"], ["a", "b"])
        self.assertEqual(first["examples"], [["a", "gbuffers_water"], ["b", "gbuffers_terrain"]])
        self.assertIn("| 1 | translate |", corpus.bucket_table(groups))

    def test_spirv_and_chrome_stages(self):
        r = result("a", [program("final", spirv={"vs": None, "fs": "Expression is not cached"},
                                 chrome={"vs": ["x"], "fs": []})])
        stages = sorted(f["stage"] for f in corpus.failures(r, DIM))
        self.assertEqual(stages, ["chrome", "spirv"])


class UnknownUniformsTest(unittest.TestCase):
    def test_counts_packs(self):
        a = result("a", [program("final")])
        a["dimensions"][DIM]["programs"][0]["unknown_uniforms"] = ["x", "y"]
        b = result("b", [program("final")])
        b["dimensions"][DIM]["programs"][0]["unknown_uniforms"] = ["y"]
        self.assertEqual(corpus.unknown_uniforms([a, b], [DIM]), [("y", ["a", "b"]), ("x", ["a"])])


class ChromeTest(unittest.TestCase):
    def test_parse_errors_only(self):
        out = ("Chrome 158\n/tmp/p/final.fs.wgsl:3:7: error: unresolved value 'x'\n"
               "/tmp/p/final.fs.wgsl:9:1: warning: unused\nFAIL /tmp/p/final.fs.wgsl\n1 of 2 compiled\n")
        self.assertEqual(corpus.parse_chrome(out), {"/tmp/p/final.fs.wgsl": ["unresolved value 'x'"]})


class TableTest(unittest.TestCase):
    def test_row_has_status_and_flags(self):
        r = result("a", [program("final", essential=True)], {"custom_uniforms": ["x", "y"]})
        text = corpus.table([corpus.pack_row(r, DIM, False)], False)
        self.assertIn("uniform:2", text)
        self.assertIn("Works-candidate", text)


class PickVersionTest(unittest.TestCase):
    def test_prefers_newest_iris(self):
        versions = [{"date_published": "2025", "loaders": ["optifine"], "id": 1},
                    {"date_published": "2024", "loaders": ["iris"], "id": 2},
                    {"date_published": "2023", "loaders": ["iris"], "id": 3}]
        self.assertEqual(corpus.pick_version(versions)["id"], 2)
        self.assertEqual(corpus.pick_version(versions[:1])["id"], 1)


if __name__ == "__main__":
    unittest.main()
