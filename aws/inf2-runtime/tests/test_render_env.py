import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("render_env", ROOT / "scripts" / "render-env.py")
render_env = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(render_env)


class RenderEnvTests(unittest.TestCase):
    def test_qwen3_4b_env(self):
        models = render_env.load_models(ROOT / "models.yaml")
        env = render_env.profile_env(models, "qwen3_4b_inf2_4k")
        self.assertEqual(env["MODEL_ID"], "Qwen/Qwen3-4B")
        self.assertEqual(env["DEVICE"], "neuron")
        self.assertEqual(env["STATUS"], "validated_target")

    def test_model_id_override(self):
        models = render_env.load_models(ROOT / "models.yaml")
        env = render_env.profile_env(models, "qwen3_4b_inf2_4k", "custom/model")
        self.assertEqual(env["MODEL_ID"], "custom/model")

    def test_qwen3_8b_inf2_32k_env(self):
        models = render_env.load_models(ROOT / "models.yaml")
        env = render_env.profile_env(models, "qwen3_8b_inf2_32k")
        self.assertEqual(env["MODEL_ID"], "Qwen/Qwen3-8B")
        self.assertEqual(env["INSTANCE_TYPE"], "inf2.8xlarge")
        self.assertEqual(env["MAX_MODEL_LEN"], "32768")
        self.assertEqual(env["MAX_NUM_SEQS"], "1")
        self.assertEqual(env["BLOCK_SIZE"], "32")
        self.assertEqual(env["NUM_GPU_BLOCKS_OVERRIDE"], "1")


if __name__ == "__main__":
    unittest.main()
