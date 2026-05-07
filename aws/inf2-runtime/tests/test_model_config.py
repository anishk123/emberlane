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
        self.assertIn("qwen25_15b", models)
        self.assertEqual(models["llama32_1b"]["model_id"], "meta-llama/Llama-3.2-1B")

    def test_qwen_is_experimental(self):
        models = render_env.load_models(ROOT / "models.yaml")
        self.assertEqual(models["qwen25_15b"]["status"], "experimental")


if __name__ == "__main__":
    unittest.main()
