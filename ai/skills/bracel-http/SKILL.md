---
name: bracel-http
description: Create or change Bracel routes, validated requests, collection queries and OpenAPI contracts.
---

1. Read `project_info` and search for Registry, validated requests and collection queries.
   Read the matching API and the application's existing route registration before editing.
2. Register handlers through the application's registry so runtime routes, policies and
   OpenAPI share the same declarations. Keep application state and domain paths in the app.
3. Preserve mandatory authorization predicates when adding filters or pagination. A cursor
   binds query context; authorization still requires application policy checks.
4. Exercise the route through HTTP for success, invalid input and denied access. Verify
   its runtime response and OpenAPI agree. Update the capability/documentation references
   and run the application's documented checks before reporting completion.
