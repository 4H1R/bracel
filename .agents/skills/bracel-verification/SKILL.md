---
name: bracel-verification
description: Validate Bracel feature changes, generated resources or upgrades through application acceptance checks.
---

Read the repository's check scripts and current capability evidence before selecting
commands. Write observable failure scenarios before implementation. Prefer public HTTP,
CLI and worker interfaces with disposable databases. For dependency changes, verify a
consumer outside the workspace. Record the resolved revision, relevant source references,
commands and outcomes in a repeatable artifact. Report failures and blocked checks separately
from passing checks; compiled support alone does not verify an external provider.
