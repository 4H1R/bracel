
CREATE TABLE bracel_idempotency (scope text NOT NULL, key text NOT NULL, fingerprint text NOT NULL, status integer NOT NULL, response jsonb NOT NULL, expires_at timestamptz NOT NULL, PRIMARY KEY(scope,key));
CREATE INDEX bracel_idempotency_expiry ON bracel_idempotency(expires_at);
CREATE TABLE bracel_memberships (tenant uuid NOT NULL, principal text NOT NULL, role text NOT NULL CHECK(role IN ('reader','writer','admin')), PRIMARY KEY(tenant,principal));
CREATE TABLE bracel_audit (id uuid PRIMARY KEY, scope text NOT NULL, actor text NOT NULL, action text NOT NULL, resource uuid NOT NULL, metadata jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE INDEX bracel_audit_scope ON bracel_audit(scope,created_at,id);