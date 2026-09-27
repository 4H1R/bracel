# Future work

These candidates preserve open ideas from earlier reviews. They are not release commitments. Consult the [package guide](packages.md), [capability catalog](guides/features/index.md), and [AI companion guide](ai.md) for current behavior.

## Realtime contention

Measure throughput and lock wait under representative concurrent event publication. The singleton `bracel_event_clock` row preserves commit-safe cursor ordering but serializes publishing transactions across scopes. If measurements show a bottleneck, evaluate per-scope ordering or an outbox publisher with an explicit cursor migration. Preserve the ordering guarantee until an alternative is verified. No contention benchmark is recorded here. See [event publication](../crates/bracel-realtime/src/lib.rs).

## Starter profiles

Consider separate minimal, account, and showcase profiles if applications need different starting points. Keep generated applications reproducible and preserve adopted and released migration history. See [starter distribution](distribution.md).

## Shared infrastructure and job workflows

Evaluate distributed cache/quotas and advanced job chains or batches when workload evidence requires them. Define cross-replica consistency, failure, retry, and cancellation behavior before adding these options. Existing named queues and calendar schedules are documented in the [package guide](packages.md).

## AI search

Consider hosted semantic search only when local search quality, distribution costs, or cross-project use justify it. Preserve exact-revision source matching and stale-context diagnostics described in the [AI companion guide](ai.md).
