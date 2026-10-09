# Optional authentication and credentials

Not implemented in the base. Add only for an actual provider/product need.

- Put validation/restoration/expiry/revocation policy in src/session/, with pure transitions in state.rs and owner-local tests. Login view owns editable inputs/focus, not tokens or session policy.
- Retain session workflows above replaceable screens. Determine whether the host is window/session/process scoped before adding multiwindow behavior.
- Bind endpoint/account/credential immutably in the connector. Publish only a verified candidate context; timeouts are not authoritative rejection. Provider endpoints and auth protocols differ; do not copy Hamlet endpoints.
- Synchronously close protected dispatch and clear authoritative sensitive state on invalidation, before asynchronous cleanup. Use session generation and request IDs to reject late outcomes; handle drop/cancellation alone is insufficient.
- Clear passwords from retained hidden controls on accepted authentication/server changes. This is lifecycle cleanup, not a memory-zeroization guarantee.
- Put provider mechanics in storage/credentials.rs on a dedicated blocking worker. Keep tokens out of config files, Debug/logs/errors and fixtures. Config stores public metadata and deletion intents only.
- Serialize credential save/metadata/delete as one protocol; a timed-out waiter does not prove a blocking job stopped. Report secure deletion and remote revocation separately, with explicit confirmed/failed/unconfirmed outcomes and restart cleanup.
- Use fake providers/temp paths in tests. Choose an appropriate OS provider for supported targets; Linux Secret Service is not automatically portable.
- Native real-keyring access requires the VERIFY.md safety gate and separate consent. Changing config directories does not isolate a shared wallet. Use a disposable OS user or demonstrably private provider D-Bus/environment before credential drills.
