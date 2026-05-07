# Future Clouds

AWS is the first implemented Emberlane cloud backend.

GCP and Azure are planned but not implemented. If a user asks for them today, Emberlane should return a clear error:

```text
GCP backend is not implemented yet. AWS is the first supported backend.
```

Likely future mapping:

- GCP: Managed Instance Groups, Cloud Run or Cloud Functions bridge, GPU instances, load balancer.
- Azure: Virtual Machine Scale Sets, Azure Functions bridge, GPU instances, load balancer.

These are future design notes, not working features.
