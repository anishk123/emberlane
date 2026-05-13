# S3 Artifact Store

Emberlane stores uploaded files locally by default. v0.3 adds an optional S3 artifact store for AWS deployments where an ASG-backed runtime or Lambda WakeBridge needs file references that do not depend on the developer machine's local disk.

## Why Local Remains Default

Local storage is the simplest path for development, echo demos, local Ollama, and MCP tools. It requires no AWS credentials and keeps files under `.emberlane/files`.

Use S3 when the runtime is remote, especially with `aws_asg`, and the runtime needs to download an uploaded file from S3 or a temporary presigned URL.

## Config

```toml
[storage]
backend = "s3"
inline_file_max_bytes = 200000

[storage.local]
data_dir = ".emberlane"

[storage.s3]
bucket = "my-emberlane-artifacts"
prefix = "uploads/"
region = "us-west-2"
aws_cli = "aws"
profile = ""
presign_downloads = true
presign_expires_secs = 900
pass_s3_uri = true
```

Emberlane uses the AWS CLI for v0.3:

- Upload: `aws s3 cp <source_path> s3://bucket/key --region <region>`
- Download for small file-chat: `aws s3 cp s3://bucket/key - --region <region>`
- Presign: `aws s3 presign s3://bucket/key --expires-in <seconds> --region <region>`

You usually do not need to hand-create the bucket. Emberlane can switch the active storage backend for you and will create the derived AWS artifact bucket on demand when credentials allow it:

```sh
cargo run -- storage use local
cargo run -- storage use s3 --profile emberlane --region us-west-2
```

Normal tests use fake command runners and do not require AWS credentials.

## Upload Flow

```sh
cargo run -- upload README.md
```

With local storage, metadata includes `storage_backend = "local"` and `stored_path`.

With S3 storage, metadata includes:

- `storage_backend = "s3"`
- `storage_key`
- `bucket`
- `region`
- `s3_uri`
- `sha256`

Object keys are generated as:

```text
prefix/yyyy/mm/dd/<file_id>/<sanitized_original_name>
```

Original filenames are never trusted as paths.

## Multiple Documents In AWS

You can upload multiple text documents at once and then ask a question about more than one uploaded file:

```sh
cargo run -- upload README.md docs/aws-deploy-from-zero.md
cargo run -- chat-files qwen25_15b_inf2_economy <file_id_1> <file_id_2> --message "compare these notes"
```

## Presigned URL Flow

```sh
cargo run -- files presign <file_id> --expires 900
```

HTTP:

```sh
curl -X POST http://127.0.0.1:8787/v1/files/<file_id>/presign \
  -H "Authorization: Bearer dev-secret" \
  -H "Content-Type: application/json" \
  -d '{"expires_secs":900}'
```

Local files return `presign_not_supported`.

## AWS ASG Runtime File Flow

Route an S3-backed file reference to a runtime:

```sh
cargo run -- files route <file_id> aws-echo --path /process-file --presign
```

The runtime receives:

```json
{
  "file": {
    "file_id": "...",
    "original_name": "README.md",
    "storage_backend": "s3",
    "s3_uri": "s3://bucket/uploads/...",
    "presigned_url": "https://...",
    "mime_type": "text/markdown",
    "size_bytes": 123,
    "sha256": "..."
  }
}
```

For safety, Emberlane does not expose local `stored_path` values to `aws_asg` runtimes. Use S3 storage for remote AWS runtimes.

## File-Context Chat

For `.txt` and `.md` files at or below `storage.inline_file_max_bytes`, Emberlane downloads or reads the file and inlines its text into the chat prompt.

For larger S3 files targeting `aws_asg`, Emberlane sends `s3_uri` and optionally `presigned_url` to the runtime instead of trying to inline the content.

Unsupported file types remain rejected for local text chat.

## Lambda WakeBridge File Flow

The recommended AWS flow is:

1. Emberlane stores an uploaded file in S3.
2. Emberlane wakes the ASG directly or sends a request through Lambda WakeBridge.
3. Emberlane includes `s3_uri` or `presigned_url` in the request body.
4. The runtime downloads the file from S3 or the presigned URL.
5. The runtime returns the result.

The Lambda WakeBridge passes JSON request bodies through unchanged, including file metadata.

## IAM Policy

Use least privilege for the bucket and prefix:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["s3:ListBucket"],
      "Resource": "arn:aws:s3:::BUCKET",
      "Condition": {
        "StringLike": {
          "s3:prefix": ["PREFIX/*"]
        }
      }
    },
    {
      "Effect": "Allow",
      "Action": ["s3:PutObject", "s3:GetObject", "s3:HeadObject"],
      "Resource": "arn:aws:s3:::BUCKET/PREFIX/*"
    }
  ]
}
```

`s3:DeleteObject` is not needed unless you add your own cleanup/delete flow.

## Security Notes

- Presigned URLs are temporary bearer URLs.
- Keep expirations short.
- Do not log presigned URLs.
- Do not expose local paths to remote runtimes.
- Enable bucket encryption.
- Use least-privilege IAM scoped to the artifact prefix.

## Limitations

- No multipart large-file upload in v0.3.
- No S3 delete lifecycle management in Emberlane yet.
- No PDF parsing.
- S3 support uses the AWS CLI in Rust v0.3, not the AWS Rust SDK.
- Local tests use fake S3 command runners.
