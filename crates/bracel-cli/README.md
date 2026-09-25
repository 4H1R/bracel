# Bracel CLI

Install from the framework workspace with cargo install --path crates/bracel-cli.
Run bracel new my-api to clone the matching versioned starter into a new directory.

Requires Git and access to https://github.com/4H1R/bracel-starter.
Existing destinations are never overwritten. The starter remote and its shallow
history remain available for reviewing future template changes.


Inside an application, run bracel make resource Project --field name:string --crud.
Use --dry-run --json to inspect the plan without writing files. Required string,
i64 and bool fields are supported. Generated code includes owner-scoped CRUD,
validation, pagination, migration registration and PostgreSQL HTTP tests.
