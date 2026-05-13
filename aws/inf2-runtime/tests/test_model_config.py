import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("render_env", ROOT / "scripts" / "render-env.py")
render_env = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(render_env)


class ModelConfigTests(unittest.TestCase):
    def test_models_yaml_parses(self):
        models = render_env.load_models(ROOT / "models.yaml")
        self.assertIn("llama32_1b", models)
        self.assertIn("qwen25_15b_inf2_economy", models)
        self.assertIn("qwen3_8b_inf2_32k", models)
        self.assertEqual(
            models["qwen25_15b_inf2_economy"]["model_id"],
            "Qwen/Qwen2.5-1.5B-Instruct",
        )
        self.assertEqual(models["llama32_1b"]["status"], "hidden")

    def test_qwen_profile_is_ready(self):
        models = render_env.load_models(ROOT / "models.yaml")
        self.assertEqual(
            models["qwen25_15b_inf2_economy"]["status"], "validated_target"
        )


if __name__ == "__main__":
    unittest.main()
