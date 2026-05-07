resource "aws_s3_bucket" "artifacts" {
  count = var.create_artifact_bucket ? 1 : 0

  bucket = local.artifact_bucket_name

  tags = merge(local.common_tags, {
    Name = local.artifact_bucket_name
  })
}

resource "aws_s3_bucket_public_access_block" "artifacts" {
  count = var.create_artifact_bucket ? 1 : 0

  bucket                  = aws_s3_bucket.artifacts[0].id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_server_side_encryption_configuration" "artifacts" {
  count = var.create_artifact_bucket ? 1 : 0

  bucket = aws_s3_bucket.artifacts[0].id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_versioning" "artifacts" {
  count = var.create_artifact_bucket ? 1 : 0

  bucket = aws_s3_bucket.artifacts[0].id

  versioning_configuration {
    status = "Enabled"
  }
}

data "archive_file" "inf2_runtime_pack" {
  type        = "zip"
  source_dir  = "${path.module}/../../aws/inf2-runtime"
  output_path = "${path.module}/inf2-runtime-pack.zip"
}

resource "aws_s3_object" "inf2_runtime_pack" {
  count  = var.create_artifact_bucket ? 1 : 0
  bucket = aws_s3_bucket.artifacts[0].id
  key    = "${local.artifact_prefix}runtime-packs/inf2-runtime-pack-${data.archive_file.inf2_runtime_pack.output_md5}.zip"
  source = data.archive_file.inf2_runtime_pack.output_path
  etag   = data.archive_file.inf2_runtime_pack.output_md5
}
