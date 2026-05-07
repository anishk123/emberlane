import base64
import json
import os
import time
import urllib.error
import urllib.request

import boto3


def handler(event, _context):
    method = _method(event)
    path = _path(event)

    if method == "GET" and path == "/healthz":
        return _json_response(200, {"ok": True})

    auth_error = _auth_error(event)
    if auth_error:
        return auth_error

    body = _json_body(event)
    if isinstance(body, dict) and body.get("stream") is True:
        return _json_response(
            400,
            {
                "ok": False,
                "error": {
                    "code": "invalid_request",
                    "message": "Streaming is not supported by Lambda WakeBridge v0.2. Use aws/lambda-bridge-node for Lambda response streaming where supported.",
                    "details": {},
                },
            },
        )

    if not _healthy():
        _wake_asg()
        if _mode() == "slow":
            return _warming()
        if not _wait_for_health():
            return _warming()

    return _proxy(method, path, body)


def _method(event):
    return (
        event.get("requestContext", {})
        .get("http", {})
        .get("method", event.get("httpMethod", "GET"))
        .upper()
    )


def _path(event):
    return event.get("rawPath") or event.get("path") or "/"


def _headers(event):
    return {str(k).lower(): str(v) for k, v in (event.get("headers") or {}).items()}


def _auth_error(event):
    api_key = os.environ.get("API_KEY", "")
    if not api_key:
        return None
    headers = _headers(event)
    got_auth = headers.get("authorization", "")
    got_api_key = headers.get("x-api-key", "")
    if got_auth == f"Bearer {api_key}" or got_api_key == api_key:
        return None
    return _json_response(
        401,
        {
            "ok": False,
            "error": {
                "code": "auth_required",
                "message": "authorization required",
                "details": {},
            },
        },
    )


def _json_body(event):
    raw = event.get("body") or "{}"
    if event.get("isBase64Encoded"):
        raw = base64.b64decode(raw).decode("utf-8")
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {}


def _base_url():
    return os.environ["BASE_URL"].rstrip("/")


def _health_url():
    return _base_url() + os.environ.get("HEALTH_PATH", "/health")


def _mode():
    return os.environ.get("MODE", "fast").lower()


def _healthy():
    try:
        with urllib.request.urlopen(_health_url(), timeout=2) as resp:
            return 200 <= resp.status < 300
    except Exception:
        return False


def _wake_asg():
    boto3.client("autoscaling", region_name=os.environ.get("AWS_REGION")).set_desired_capacity(
        AutoScalingGroupName=os.environ["ASG_NAME"],
        DesiredCapacity=int(os.environ.get("DESIRED_CAPACITY_ON_WAKE", "1")),
        HonorCooldown=False,
    )


def _wait_for_health():
    deadline = time.time() + int(os.environ.get("FAST_WAIT_SECS", "25"))
    startup_deadline = time.time() + int(os.environ.get("STARTUP_TIMEOUT_SECS", "180"))
    deadline = min(deadline, startup_deadline)
    while time.time() < deadline:
        if _healthy():
            return True
        time.sleep(1)
    return False


def _warming():
    retry_after = int(os.environ.get("RETRY_AFTER_SECS", "5"))
    return _json_response(
        202,
        {
            "ok": True,
            "data": {
                "state": "waking",
                "message": "Runtime is warming",
                "retry_after_secs": retry_after,
            },
        },
        {"Retry-After": str(retry_after)},
    )


def _proxy(method, path, body):
    data = None if method == "GET" else json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        _base_url() + path,
        data=data,
        method=method,
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=int(os.environ.get("PROXY_TIMEOUT_SECS", "60"))) as resp:
            raw = resp.read().decode("utf-8")
            return _raw_response(resp.status, raw, dict(resp.headers))
    except urllib.error.HTTPError as err:
        raw = err.read().decode("utf-8")
        return _raw_response(err.code, raw, dict(err.headers))
    except Exception as err:
        return _json_response(
            502,
            {
                "ok": False,
                "error": {
                    "code": "route_failed",
                    "message": f"failed to proxy request: {err}",
                    "details": {},
                },
            },
        )


def _raw_response(status, raw, upstream_headers):
    content_type = upstream_headers.get("content-type") or upstream_headers.get("Content-Type") or "application/json"
    return {
        "statusCode": status,
        "headers": {"Content-Type": content_type},
        "body": raw,
        "isBase64Encoded": False,
    }


def _json_response(status, body, headers=None):
    merged = {"Content-Type": "application/json"}
    if headers:
        merged.update(headers)
    return {
        "statusCode": status,
        "headers": merged,
        "body": json.dumps(body),
        "isBase64Encoded": False,
    }
