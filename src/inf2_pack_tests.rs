use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::Command,
    thread,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    fs::read_to_string(root().join(path)).unwrap()
}

#[test]
fn inf2_runtime_pack_files_exist_and_models_are_defined() {
    for path in [
        "aws/inf2-runtime/README.md",
        "aws/inf2-runtime/Dockerfile.neuron",
        "aws/inf2-runtime/bootstrap.sh",
        "aws/inf2-runtime/start-server.sh",
        "aws/inf2-runtime/healthcheck.sh",
        "aws/inf2-runtime/models.yaml",
        "aws/inf2-runtime/systemd/emberlane-inf2.service",
        "aws/inf2-runtime/nginx/nginx.conf",
        "aws/inf2-runtime/scripts/render-env.py",
        "aws/inf2-runtime/server/health_proxy.py",
    ] {
        assert!(root().join(path).exists(), "{path} should exist");
    }
    let models = read("aws/inf2-runtime/models.yaml");
    assert!(models.contains("llama32_1b"));
    assert!(models.contains("meta-llama/Llama-3.2-1B"));
    assert!(models.contains("status: \"validated_target\""));
    assert!(models.contains("qwen25_15b"));
    assert!(models.contains("Qwen/Qwen2.5-1.5B-Instruct"));
    assert!(models.contains("status: \"experimental\""));
    assert!(models.contains("qwen3_4b"));
    assert!(models.contains("Qwen/Qwen3-4B-Instruct-2507"));
    assert!(models.contains("qwen3_8b_inf2_4k"));
    assert!(models.contains("Qwen/Qwen3-8B"));
}

#[test]
fn render_env_outputs_llama_and_qwen_profiles() {
    let script = root().join("aws/inf2-runtime/scripts/render-env.py");
    let llama = Command::new("python3")
        .arg(&script)
        .arg("--profile")
        .arg("llama32_1b")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(llama.status.success());
    let text = String::from_utf8(llama.stdout).unwrap();
    assert!(text.contains("\"MODEL_ID\": \"meta-llama/Llama-3.2-1B\""));
    assert!(text.contains("\"TENSOR_PARALLEL_SIZE\": \"2\""));
    assert!(text.contains("\"STATUS\": \"validated_target\""));

    let qwen = Command::new("python3")
        .arg(&script)
        .arg("--profile")
        .arg("qwen25_15b")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(qwen.status.success());
    let text = String::from_utf8(qwen.stdout).unwrap();
    assert!(text.contains("Qwen/Qwen2.5-1.5B-Instruct"));
    assert!(text.contains("\"STATUS\": \"experimental\""));

    let qwen3 = Command::new("python3")
        .arg(&script)
        .arg("--profile")
        .arg("qwen3_4b")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(qwen3.status.success());
    let text = String::from_utf8(qwen3.stdout).unwrap();
    assert!(text.contains("Qwen/Qwen3-4B-Instruct-2507"));
    assert!(text.contains("\"INSTANCE_TYPE\": \"inf2.xlarge\""));
    assert!(text.contains("\"STATUS\": \"experimental\""));

    let qwen3_8b = Command::new("python3")
        .arg(&script)
        .arg("--profile")
        .arg("qwen3_8b_inf2_4k")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(qwen3_8b.status.success());
    let text = String::from_utf8(qwen3_8b.stdout).unwrap();
    assert!(text.contains("Qwen/Qwen3-8B"));
    assert!(text.contains("\"INSTANCE_TYPE\": \"inf2.xlarge\""));
    assert!(text.contains("\"MAX_MODEL_LEN\": \"4096\""));
    assert!(text.contains("\"MAX_NUM_SEQS\": \"8\""));
    assert!(text.contains("\"BLOCK_SIZE\": \"32\""));
    assert!(text.contains("\"NUM_GPU_BLOCKS_OVERRIDE\": \"8\""));
}

#[test]
fn start_server_contains_required_vllm_neuron_flags() {
    let script = read("aws/inf2-runtime/start-server.sh");
    for needle in [
        "vllm serve",
        "--device neuron",
        "--tensor-parallel-size",
        "--block-size",
        "--max-model-len",
        "--max-num-seqs",
        "--num-gpu-blocks-override",
        "--no-enable-prefix-caching",
        "--host 0.0.0.0",
        "--port",
        "S3_NEURON_ARTIFACTS_URI",
        "SYNC_ARTIFACTS_BACK",
    ] {
        assert!(script.contains(needle), "missing {needle}");
    }
}

#[test]
fn health_proxy_reports_503_when_upstream_not_ready_and_200_when_ready() {
    let script = root().join("aws/inf2-runtime/server/health_proxy.py");
    let check = |url: &str| {
        let code = format!(
            "import importlib.util; spec=importlib.util.spec_from_file_location('h', r'{}'); h=importlib.util.module_from_spec(spec); spec.loader.exec_module(h); h.UPSTREAM_MODELS_URL='{}'; print(200 if h.model_server_ready(timeout=1) else 503)",
            script.display(),
            url
        );
        Command::new("python3")
            .arg("-c")
            .arg(code)
            .output()
            .unwrap()
    };

    let closed = check("http://127.0.0.1:9/v1/models");
    assert_eq!(String::from_utf8(closed.stdout).unwrap().trim(), "503");

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0; 512];
        let _ = stream.read(&mut buf);
        let body = "{\"object\":\"list\",\"data\":[]}";
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    let ready = check(&format!("http://{addr}/v1/models"));
    assert_eq!(String::from_utf8(ready.stdout).unwrap().trim(), "200");
}

#[test]
fn node_lambda_bridge_contains_auth_warming_and_streaming_logic() {
    let node = read("aws/lambda-bridge-node/index.mjs");
    assert!(node.contains("isAuthorized"));
    assert!(node.contains("warmingResponse"));
    assert!(node.contains("awslambda.streamifyResponse"));
    assert!(node.contains("text/event-stream"));
    assert!(node.contains("SetDesiredCapacityCommand"));

    let template = read("aws/lambda-bridge-node/template.yaml");
    assert!(template.contains("InvokeMode: RESPONSE_STREAM"));
    assert!(template.contains("autoscaling:SetDesiredCapacity"));
}

#[test]
fn docs_and_config_include_inf2_llama() {
    assert!(read("emberlane.example.toml").contains("id = \"inf2-llama\""));
    assert!(read("docs/inf2-runtime.md").contains("meta-llama/Llama-3.2-1B"));
    assert!(read("docs/aws-end-to-end.md").contains("Lambda VPC streaming limitation"));
    assert!(read("docs/inf2-runtime.md").contains("qwen3_4b"));
    assert!(read("docs/inf2-runtime.md").contains("qwen3_8b_inf2_4k"));
}

#[allow(dead_code)]
fn _assert_tcp_stream_send(_: TcpStream) {}
