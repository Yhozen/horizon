#!/usr/bin/env python3
"""Serve the browser build with cross-origin isolation (COOP/COEP).

The threaded wasm build shares its memory with Web Workers (SharedArrayBuffer), which
browsers allow only on a cross-origin isolated page. Plain `python3 -m http.server`
does not send these headers.

    python3 tools/web/serve.py [--port 8080] [--bind 127.0.0.1] [--dir web]
"""
import argparse
import functools
import http.server


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
        ".mjs": "text/javascript",
    }

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
    p.add_argument("--dir", default="web")
    a = p.parse_args()
    handler = functools.partial(Handler, directory=a.dir)
    server = http.server.ThreadingHTTPServer((a.bind, a.port), handler)
    print(f"Serving {a.dir} on http://{a.bind}:{a.port}/ (cross-origin isolated)")
    server.serve_forever()


if __name__ == "__main__":
    main()
