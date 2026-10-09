# Optional owned backend

Not implemented in the base. A desktop app using local files or third-party providers need not grow a server.

When a product owns a backend contract, consider separate protocol and server crates plus the GUI crate. Decide explicitly whether a Cargo workspace earns its keep. Shared protocol owns serde request/response/event data, not GUI/workflow state; reuse exact shapes and convert only where differences serve a purpose.

Keep feature-local route declarations, handlers, DTO integration and operations together. Put shared auth/error/request-ID HTTP mechanics in a focused shared module; process startup constructs dependencies, reusable library wiring assembles routes. Schema migrations own persistent schema changes, not ad hoc startup SQL.

Maintain contract/OpenAPI assembly and route inventory when routes/DTOs change. Test public-interface HTTP scenarios against disposable databases and ephemeral loopback ports; feature-owned publication logic uses owner-local tests. Desktop real-route scenarios belong to the owning feature, not every connector test.

Define admission/authorization, TLS, rate limits, token/cookie/CSRF policy and error/secret logging deliberately. An in-process event hub is not durable replay or multi-instance delivery. Do not imply synchronization/outbox guarantees from a successful HTTP response alone.

Use worktree-owned databases/endpoints; never inherit another project's database URL or redirect tests to an existing server.
