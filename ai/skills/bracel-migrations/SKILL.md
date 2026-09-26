---
name: bracel-migrations
description: Add Bracel application database migrations or adopt optional framework persistence.
---

Read current migration registration and the selected package's schema requirements.
Applications own migration history. Add a forward migration for released schemas and
check startup behavior with pending migrations. Use disposable PostgreSQL to verify a
fresh database and an upgrade from the prior schema. Inspect and migrate are separate
operations; run migrations only within the user's authorized development workflow.
