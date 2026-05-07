import importlib.util
import json
import os
import sys
import types
import unittest
from pathlib import Path
from unittest.mock import patch


class FakeAutoScaling:
    def __init__(self):
        self.calls = []

    def set_desired_capacity(self, **kwargs):
        self.calls.append(kwargs)


class LambdaBridgeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fake_autoscaling = FakeAutoScaling()
        fake_boto3 = types.SimpleNamespace(
            client=lambda service, region_name=None: cls.fake_autoscaling
        )
        sys.modules["boto3"] = fake_boto3
        app_path = Path(__file__).resolve().parents[1] / "app.py"
        spec = importlib.util.spec_from_file_location("lambda_bridge_app", app_path)
        cls.app = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.app)

    def setUp(self):
        self.fake_autoscaling.calls.clear()
        os.environ.clear()
        os.environ.update(
            {
                "AWS_REGION": "us-west-2",
                "ASG_NAME": "emberlane-test-asg",
                "BASE_URL": "http://runtime.example",
                "HEALTH_PATH": "/health",
                "RETRY_AFTER_SECS": "5",
                "DESIRED_CAPACITY_ON_WAKE": "1",
            }
        )

    def event(self, path="/v1/chat/completions", body=None, headers=None):
        return {
            "version": "2.0",
            "rawPath": path,
            "headers": headers or {},
            "requestContext": {"http": {"method": "POST", "path": path}},
            "body": json.dumps(body or {"messages": []}),
            "isBase64Encoded": False,
        }

    def test_healthz_does_not_require_auth_or_wake(self):
        os.environ["API_KEY"] = "secret"
        response = self.app.handler(
            {
                "rawPath": "/healthz",
                "headers": {},
                "requestContext": {"http": {"method": "GET", "path": "/healthz"}},
            },
            None,
        )
        self.assertEqual(response["statusCode"], 200)
        self.assertEqual(self.fake_autoscaling.calls, [])

    def test_auth_required_when_api_key_is_set(self):
        os.environ["API_KEY"] = "secret"
        response = self.app.handler(self.event(headers={}), None)
        self.assertEqual(response["statusCode"], 401)
        self.assertIn("auth_required", response["body"])

    def test_x_api_key_is_accepted_when_api_key_is_set(self):
        os.environ["API_KEY"] = "secret"
        with patch.object(self.app, "_healthy", return_value=True), patch.object(
            self.app, "_proxy", return_value={"statusCode": 200, "body": "{}"}
        ) as proxy:
            response = self.app.handler(
                self.event(headers={"x-api-key": "secret"}),
                None,
            )
        self.assertEqual(response["statusCode"], 200)
        proxy.assert_called_once()

    def test_streaming_returns_v02_error(self):
        response = self.app.handler(self.event(body={"stream": True}), None)
        self.assertEqual(response["statusCode"], 400)
        self.assertIn("Streaming is not supported by Lambda WakeBridge v0.2", response["body"])

    def test_slow_mode_wakes_and_returns_warming(self):
        os.environ["MODE"] = "slow"
        with patch.object(self.app, "_healthy", return_value=False):
            response = self.app.handler(self.event(), None)
        self.assertEqual(response["statusCode"], 202)
        self.assertIn("Runtime is warming", response["body"])
        self.assertEqual(
            self.fake_autoscaling.calls[0]["AutoScalingGroupName"], "emberlane-test-asg"
        )


if __name__ == "__main__":
    unittest.main()
