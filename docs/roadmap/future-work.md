# Future Work

These ideas are useful, but they are not supported active features in the public alpha.

## Planned: Python And TypeScript SDKs

Python and TypeScript SDKs are planned after the CLI, MCP stdio, and HTTP/OpenAI-compatible API are stable.

Before SDKs become supported, they should include:

- packaged client code
- tests
- README usage examples
- CI coverage or a documented validation path
- compatibility with the supported HTTP API

## Future Cloud Backends

AWS is the first implemented cloud backend. GCP and Azure are future backends only.

Possible future mappings:

- GCP: Managed Instance Groups, Cloud Run or Cloud Functions bridge, GPU instances, and load balancer.
- Azure: Virtual Machine Scale Sets, Azure Functions bridge, GPU instances, and load balancer.

## Ideas Not In Core Today

- RAG worker examples.
- Search worker examples.
- Plugin provider systems.
- Dashboards.
- Production hosted service.

These should stay outside active docs and examples until they are real, tested, and central to Emberlane.
