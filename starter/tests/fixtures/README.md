# Public JWT test fixtures

This RSA pair is generated solely for automated tests and is intentionally public. It is used by `tests/middleware.rs` and `scripts/container-smoke.sh`. Never configure it as a deployment's issuer key. The Docker build context excludes this directory; the smoke test passes the public half explicitly to the disposable container. No production key or token is stored here.
