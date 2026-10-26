#!/usr/bin/env python3
"""Serve the browser build with cross-origin isolation (COOP/COEP).

A packaged release keeps its page shell in site/ and its hashed assets in objects/.
Serve both at their expected URLs, with the COOP/COEP headers needed for browser
experiments that use SharedArrayBuffer.

    python3 tools/web/serve.py [--port 8080] [--bind 127.0.0.1] [--dir web/.build]
"""
import argparse
import functools
import http.server
from pathlib import Path
import re
from urllib.parse import urlsplit


BUILD_DIR = Path(__file__).resolve().parents[2] / "web" / ".build"
SHELL_PATHS = {"/", "/index.html", "/loader.js", "/cache.js", "/release.js", "/release.json"}
ASSET_PATH = re.compile(r"/assets/[a-f0-9]{64}/(?:horizon-data\.bin|horizon-client_bg\.wasm|horizon-client\.js)")


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
        ".mjs": "text/javascript",
    }

    def send_head(self):
        path = urlsplit(self.path).path
        if path not in SHELL_PATHS and not ASSET_PATH.fullmatch(path):
            self.send_error(404)
            return None
        return super().send_head()

    def translate_path(self, path):
        path = urlsplit(path).path
        if path == "/":
            path = "/index.html"
        folder = "objects" if ASSET_PATH.fullmatch(path) else "site"
        return super().translate_path(f"/{folder}{path}")

    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cross-Origin-Resource-Policy", "same-origin")
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--port", type=int, default=8080)
    p.add_argument("--bind", default="127.0.0.1")
    p.add_argument("--dir", type=Path, default=BUILD_DIR, help="packaged build directory (default: web/.build)")
    a = p.parse_args()
    handler = functools.partial(Handler, directory=a.dir)
    server = http.server.ThreadingHTTPServer((a.bind, a.port), handler)
    print(f"Serving {a.dir} on http://{a.bind}:{a.port}/ (cross-origin isolated)")
    server.serve_forever()


if __name__ == "__main__":
    main()
