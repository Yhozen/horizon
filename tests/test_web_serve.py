"""The Python dev server serves the packager's shell and object layout."""
import functools
import http.client
import http.server
import json
from pathlib import Path
import tempfile
import threading
import unittest

from tools.web.serve import Handler


class QuietHandler(Handler):
    def log_message(self, *_args):
        pass


class WebServeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="horizon-web-serve-")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        site = root / "site"
        site.mkdir()
        self.assets = {}
        for name in ("horizon-client.js", "horizon-client_bg.wasm", "horizon-data.bin"):
            url = f"/assets/{'a' * 64}/{name}"
            file = root / "objects" / url.lstrip("/")
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(name.encode())
            self.assets[name] = {"url": url}
        (site / "index.html").write_text("<script src='/loader.js'></script>")
        (site / "loader.js").write_text("fetch('/release.json')")
        (site / "release.json").write_text(json.dumps({"assets": self.assets}))
        # Internal packaging files must not become public URLs.
        (root / "upload.json").write_text("internal")
        self.server = http.server.ThreadingHTTPServer(
            ("127.0.0.1", 0), functools.partial(QuietHandler, directory=root)
        )
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.stop)

    def stop(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def request(self, path, method="GET"):
        conn = http.client.HTTPConnection("127.0.0.1", self.server.server_port, timeout=5)
        try:
            conn.request(method, path)
            response = conn.getresponse()
            return response.status, response.headers, response.read()
        finally:
            conn.close()

    def test_shell_manifest_and_every_asset_load(self):
        for path in ("/", "/loader.js"):
            self.assertEqual(self.request(path)[0], 200)
        status, headers, data = self.request("/release.json")
        self.assertEqual(status, 200)
        self.assertEqual(headers["Cross-Origin-Opener-Policy"], "same-origin")
        self.assertEqual(headers["Cross-Origin-Embedder-Policy"], "require-corp")
        for name, asset in json.loads(data)["assets"].items():
            with self.subTest(name=name):
                status, headers, data = self.request(asset["url"])
                self.assertEqual((status, data), (200, name.encode()))
                if name.endswith(".wasm"):
                    self.assertEqual(headers["Content-Type"], "application/wasm")
                status, headers, data = self.request(asset["url"], "HEAD")
                self.assertEqual((status, data), (200, b""))
                self.assertEqual(int(headers["Content-Length"]), len(name))

    def test_internal_paths_and_directories_are_not_served(self):
        for path in ("/upload.json", "/site/release.json", "/assets/", "/../upload.json",
                     "/assets/" + "a" * 64 + "/../../upload.json"):
            with self.subTest(path=path):
                self.assertEqual(self.request(path)[0], 404)
