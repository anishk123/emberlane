#!/usr/bin/env python3
import json
import os
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


UPSTREAM_BASE_URL = os.environ.get("UPSTREAM_BASE_URL", "http://127.0.0.1:8000").rstrip("/")
UPSTREAM_MODELS_URL = os.environ.get("UPSTREAM_MODELS_URL", f"{UPSTREAM_BASE_URL}/v1/models")
HOST = os.environ.get("PROXY_HOST", "0.0.0.0")
PORT = int(os.environ.get("RUNTIME_PORT", os.environ.get("PROXY_PORT", "8080")))


def model_server_ready(timeout=2):
    try:
        with urllib.request.urlopen(UPSTREAM_MODELS_URL, timeout=timeout) as resp:
            return 200 <= resp.status < 300
    except Exception:
        return False


def _proxy_url(path):
    return f"{UPSTREAM_BASE_URL}{path}"


def _copy_headers(handler):
    headers = {}
    for key, value in handler.headers.items():
        if key.lower() not in {"host", "content-length", "connection"}:
            headers[key] = value
    return headers


def _proxy_request(handler):
    body = None
    if handler.command != "GET":
        length = int(handler.headers.get("content-length", "0"))
        body = handler.rfile.read(length) if length > 0 else None

    req = urllib.request.Request(
        _proxy_url(handler.path),
        data=body,
        method=handler.command,
        headers=_copy_headers(handler),
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as upstream:
            payload = upstream.read()
            handler.send_response(upstream.status)
            for key, value in upstream.headers.items():
                if key.lower() not in {"transfer-encoding", "connection"}:
                    handler.send_header(key, value)
            handler.send_header("content-length", str(len(payload)))
            handler.end_headers()
            handler.wfile.write(payload)
    except urllib.error.HTTPError as err:
        payload = err.read()
        handler.send_response(err.code)
        for key, value in err.headers.items():
            if key.lower() not in {"transfer-encoding", "connection"}:
                handler.send_header(key, value)
        handler.send_header("content-length", str(len(payload)))
        handler.end_headers()
        handler.wfile.write(payload)
    except Exception as err:
        payload = json.dumps({"error": str(err)}).encode("utf-8")
        handler.send_response(502)
        handler.send_header("content-type", "application/json")
        handler.send_header("content-length", str(len(payload)))
        handler.end_headers()
        handler.wfile.write(payload)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, _fmt, *_args):
        return

    def do_GET(self):
        if self.path == "/health":
            if model_server_ready():
                self._json({"ok": True, "upstream": UPSTREAM_MODELS_URL}, 200)
            else:
                self._json({"ok": False, "state": "warming", "upstream": UPSTREAM_MODELS_URL}, 503)
            return
        _proxy_request(self)

    def do_POST(self):
        if self.path == "/health":
            self._json({"ok": False, "error": "method not allowed"}, 405)
            return
        _proxy_request(self)

    def do_PUT(self):
        _proxy_request(self)

    def do_PATCH(self):
        _proxy_request(self)

    def do_DELETE(self):
        _proxy_request(self)

    def _json(self, body, status):
        payload = json.dumps(body).encode("utf-8")
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def main():
    ThreadingHTTPServer((HOST, PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
