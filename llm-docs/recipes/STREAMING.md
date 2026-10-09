# Optional streams

Not implemented in the base; do not add a global event bus just for streams.

A connector owns one cancel-on-drop stream attempt: endpoint/auth binding, parsing, read/ready deadlines and bounded deliveries. The feature owner owns attempts, readiness, reconnect timing, reconciliation and uncertainty. Views observe state; they do not own streams or reconnect tasks.

Specify backpressure/overflow and prioritize terminal outcomes when full. Separate attempt identity from session/read/write identity. Exactly one stable host consumes outcomes; view subscriptions are invalidations, not competing result receivers.

Choose semantics explicitly: best-effort (missed events may remain absent) or replay/gap repair (cursors, ordering, recovery and server support). Hamlet's three-second reconnect without catch-up reads is one product policy, not a GPUI requirement. Stream readiness does not prove synchronization, and reconnect must not automatically replay ambiguous writes.

Test cancellation/drop, stale attempts, read overlap, overflow, terminal delivery, outage/reconnect and write uncertainty with controlled requests/time plus disposable real transport. Document persistent-state/viewport effects and deployment constraints. No production account or traffic capture in automated fixtures.
