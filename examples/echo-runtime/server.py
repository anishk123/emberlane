#!/usr/bin/env python3
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def read_json(handler):
    length = int(handler.headers.get("content-length", "0"))
    if length == 0:
        return {}
    return json.loads(handler.rfile.read(length).decode("utf-8"))


def send_json(handler, data, status=200):
    body = json.dumps(data).encode("utf-8")
    handler.send_response(status)
    handler.send_header("content-type", "application/json")
    handler.send_header("content-length", str(len(body)))
    handler.end_headers()
    handler.wfile.write(body)


def last_user_message(messages):
    return next(
        (m.get("content", "") for m in reversed(messages) if m.get("role") == "user"),
        "",
    )


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        return

    def do_GET(self):
        if self.path == "/health":
            send_json(self, {"ok": True})
        else:
            send_json(self, {"error": "not found"}, 404)

    def do_POST(self):
        body = read_json(self)
        if self.path == "/chat":
            send_json(self, {"reply": f"Echo: {last_user_message(body.get('messages', []))}"})
        elif self.path == "/echo":
            send_json(self, body)
        elif self.path == "/v1/chat/completions":
            if body.get("stream"):
                send_json(self, {"error": "streaming is not implemented in Emberlane v0.1"}, 400)
                return
            message = last_user_message(body.get("messages", []))
            send_json(
                self,
                {
                    "id": "chatcmpl-echo",
                    "object": "chat.completion",
                    "model": body.get("model", "echo"),
                    "choices": [
                        {
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": f"Echo: {message}",
                            },
                            "finish_reason": "stop",
                        }
                    ],
                },
            )
        else:
            send_json(self, {"error": "not found"}, 404)


if __name__ == "__main__":
    port = int(os.environ.get("PORT", "9001"))
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
