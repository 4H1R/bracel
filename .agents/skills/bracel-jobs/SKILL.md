---
name: bracel-jobs
description: Implement Bracel durable jobs, retries, queues, scheduled work and transactional delivery.
---

1. Resolve the installed jobs capability and search its current enqueue, worker and schedule
   APIs. Read the application's worker registration and migration history.
2. Put domain handlers in the application. Record job intent in the same transaction as
   the business change when both must succeed together. Design handlers for repeated delivery.
3. Make the retry policy, idempotency, lease behavior and terminal failure handling explicit.
   Read the selected revision's scheduling semantics before adding periodic work.
4. Verify committed work is delivered and rolled-back work is absent. Exercise retries and
   duplicate delivery through the public application/worker interface with disposable storage.
   Update docs and evidence with the commands and results actually observed.
