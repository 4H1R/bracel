# Scheduling durable work

The normal starter supports persisted intervals and calendar cron with IANA
timezones. Define application schedules in `src/schedules.rs` and register their
job handlers in `src/jobs.rs`. The example API packages are optional.

Run `migrate`, then `schedule:sync` to install definitions. Keep `schedule:work`
and `jobs:work` running as separate processes. `schedule:tick` polls both schedule
types once. `schedule:sync` preserves due times, re-enables configured schedules,
and disables removed definitions in the reserved `app.` namespace atomically.

The supplied schedule queues bounded account cleanup hourly in UTC. Expressions
include seconds. Calendar polling coalesces missed runs, avoids enqueueing while
the previous occurrence is pending/running, and disables finite schedules when
no future occurrence exists. Row locks coordinate concurrent schedulers.

Jobs retain at-least-once delivery semantics: an expired execution lease can
repeat an external effect. Keep handlers idempotent. Timezone and DST behavior
follows the pinned cron/chrono-tz implementation; review business schedules at
both clock transitions. Monitor scheduler progress and oldest pending job age.

See [configuration and commands](../helpers.md#jobs-and-cron), [jobs](jobs.md),
and [verification](../verification.md) for recorded execution evidence.
