# Security

Please do not report security issues publicly.

Use GitHub Security Advisories for this repository when available, or contact the maintainers at `security@example.com` until a project-specific security address is published.

## Sensitive Data

Emberlane can proxy requests, start local processes, invoke AWS CLI commands, generate presigned URLs, and render Terraform variables. Treat configuration and logs as infrastructure-sensitive.

Never commit or paste:

- AWS access keys, session tokens, or SSO cache contents.
- API keys or bearer tokens.
- Hugging Face tokens.
- Terraform state files containing secrets.
- Presigned URLs.
- Runtime request bodies containing private data.

## AWS Credential Handling

Emberlane should not ask users to type raw AWS secrets into its config. Prefer AWS CLI profiles, SSO, instance roles, or environment variables resolved by AWS tooling.

If you add AWS-facing code, make sure it works with:

- Default AWS profiles.
- Named profiles.
- AWS SSO profiles.
- Environment-based credentials.

Normal tests must not require AWS credentials.

## Redaction Expectations

Logs, diagnostics, reports, and test artifacts should redact values whose keys contain:

- `authorization`
- `api_key`
- `secret`
- `token`
- `password`
- `presigned_url`

Presigned URLs are temporary bearer credentials. Keep expirations short and do not log them.

## Runtime And Network Risks

Runtime configs may execute local commands or route traffic to configured HTTP endpoints. Review untrusted configs before running them. Binding Emberlane or AWS runtime endpoints publicly requires authentication, TLS, network controls, and careful logging.
