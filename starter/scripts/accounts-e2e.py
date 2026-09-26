"""Real-process account acceptance. Requires a built binary and disposable PostgreSQL."""
import concurrent.futures
import email
import email.policy
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import socket
import socketserver
import subprocess
import threading
import time
import traceback
import urllib.parse
import uuid

ROOT = Path(__file__).resolve().parents[1]
WORKSPACE = ROOT.parent if (ROOT.parent / "crates").is_dir() else ROOT
binary = Path(os.environ.get("CARGO_TARGET_DIR", WORKSPACE / "target")) / "debug" / (
    "bracel-starter.exe" if os.name == "nt" else "bracel-starter"
)
run_id = uuid.uuid4().hex
artifact = WORKSPACE / ".scratch/accounts-e2e" / run_id
artifact.mkdir(parents=True)
report = {"ok": False, "assertions": [], "reproduce": "bash scripts/accounts-e2e.sh"}
transcript, processes, logs, messages, secrets = [], [], [], [], []
schema = "accounts_" + run_id
password = "a long initial password for tests"
new_password = "a different strong password for tests"
secrets.extend([password, new_password])
smtp = None

def expect(condition, message):
    report["assertions"].append({"description": message, "passed": bool(condition)})
    assert condition, message

def command(args, env):
    return subprocess.run([str(a) for a in args], env=env, capture_output=True, text=True, check=True).stdout

def sql(statement):
    return command(["psql", "-XAt", "-v", "ON_ERROR_STOP=1", "-c", f"SET search_path={schema}; {statement}"], db_env).strip().splitlines()[-1]

def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]

def start(settings):
    port = free_port()
    log = (artifact / f"server-{len(processes)}.log").open("w")
    logs.append(log)
    proc = subprocess.Popen([binary, "serve"], env=dict(settings, BIND_ADDR=f"127.0.0.1:{port}"), stdout=log, stderr=log)
    processes.append(proc)
    for _ in range(100):
        if proc.poll() is not None:
            raise RuntimeError("server failed to start; see retained log")
        try:
            if request(port, "GET", "/healthz")[0] == 200:
                return port
        except OSError:
            pass
        time.sleep(.1)
    raise RuntimeError("server startup timeout")

def redact(value):
    if isinstance(value, dict):
        return {k: "[redacted]" if k in {"access_token", "token", "password"} else redact(v) for k, v in value.items()}
    if isinstance(value, list):
        return [redact(v) for v in value]
    return value

def request(port, method, path, body=None, token=None, raw=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    client = http.client.HTTPConnection("127.0.0.1", port, timeout=20)
    client.request(method, path, raw if raw is not None else json.dumps(body) if body is not None else None, headers)
    response = client.getresponse()
    data = response.read()
    value = json.loads(data) if data else None
    result = response.status, value, dict(response.getheaders())
    transcript.append({"method": method, "path": path, "status": response.status, "response": redact(value)})
    client.close()
    return result

class SMTP(socketserver.StreamRequestHandler):
    reject = False
    def handle(self):
        self.wfile.write(b"220 localhost ESMTP\r\n")
        while line := self.rfile.readline():
            if line.upper().startswith(b"EHLO"):
                self.wfile.write(b"250-localhost\r\n250 8BITMIME\r\n")
            elif line.upper().startswith(b"DATA"):
                self.wfile.write(b"354 continue\r\n")
                data = b""
                while (part := self.rfile.readline()) not in (b".\r\n", b""):
                    data += part
                if self.reject:
                    self.wfile.write(b"451 temporary failure\r\n")
                else:
                    messages.append(data)
                    self.wfile.write(b"250 accepted\r\n")
            elif line.upper().startswith(b"QUIT"):
                self.wfile.write(b"221 bye\r\n")
                break
            else:
                self.wfile.write(b"250 ok\r\n")

def issue(email_address="user@example.test", candidate=password):
    status, body, _ = request(api, "POST", "/api/auth/login", {"email": email_address, "password": candidate})
    expect(status == 200, "valid password creates a bearer session")
    result = body["data"]["access_token"]
    secrets.append(result)
    return result

def deliver(label="Reset"):
    command([binary, "auth:mail-once"], env)
    parsed = email.message_from_bytes(messages[-1], policy=email.policy.default)
    content = parsed.get_body(preferencelist=("plain",)).get_content()
    token = re.search(label + r" token: ([a-f0-9]{64})", content).group(1)
    secrets.append(token)
    return token

try:
    report["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
    report["lock_sha256"] = hashlib.sha256((WORKSPACE / "Cargo.lock").read_bytes()).hexdigest()
    digest = hashlib.sha256()
    for path in sorted((ROOT / "src").rglob("*.rs")):
        digest.update(str(path.relative_to(ROOT)).encode()); digest.update(path.read_bytes())
    report["source_sha256"] = digest.hexdigest()
    database = urllib.parse.urlsplit(os.environ["TEST_DATABASE_URL"])
    db_env = dict(os.environ, PGHOST=database.hostname, PGPORT=str(database.port or 5432), PGDATABASE=database.path.lstrip("/"), PGUSER=urllib.parse.unquote(database.username or ""), PGPASSWORD=urllib.parse.unquote(database.password or ""))
    command(["psql", "-X", "-v", "ON_ERROR_STOP=1", "-c", f"CREATE SCHEMA {schema}"], db_env)
    query = urllib.parse.parse_qsl(database.query) + [("options", f"-csearch_path={schema}")]
    scoped = urllib.parse.urlunsplit(database._replace(query=urllib.parse.urlencode(query)))
    smtp = socketserver.ThreadingTCPServer(("127.0.0.1", 0), SMTP)
    threading.Thread(target=smtp.serve_forever, daemon=True).start()
    env = {k: v for k, v in os.environ.items() if not k.startswith(("AUTH_", "MAIL_", "OTLP_"))}
    env.update(DATABASE_URL=scoped, AUTH_MODE="local", ENABLE_ACCOUNTS="true", ALLOW_REGISTRATION="true", ENABLE_BATTERIES="false", ENABLE_EXAMPLE="false", MAIL_LOCAL_PORT=str(smtp.server_address[1]), RATE_ANONYMOUS_PER_MINUTE="10000", RATE_AUTHENTICATED_PER_MINUTE="10000", RATE_WRITES_PER_MINUTE="10000")
    command([binary, "migrate"], env); command([binary, "migrate"], env)
    # Reconstruct the immediately preceding schema and upgrade an existing user.
    legacy_id = str(uuid.uuid4())
    sql("DROP TABLE account_verifications; ALTER TABLE users DROP COLUMN email_verified_at; DELETE FROM seaql_migrations WHERE version='m20260927_000007_email_verification'")
    sql(f"INSERT INTO users(id,email,display_name,password_hash) VALUES('{legacy_id}','legacy@example.test','Legacy','fixture-hash')")
    command([binary, "migrate"], env)
    expect(sql(f"SELECT email_verified_at IS NULL AND display_name='Legacy' FROM users WHERE id='{legacy_id}'") == "t", "verification migration preserves existing users without trusting their email")
    sql(f"DELETE FROM users WHERE id='{legacy_id}'")
    # Exercise the actual starter default, without sourcing .env.example.
    env.pop("AUTH_MODE")
    api = start(env)
    expect(request(api, "GET", "/readyz")[0] == 200, "default account schema is ready")
    sql("ALTER TABLE notes RENAME TO notes_hidden")
    expect(request(api, "GET", "/readyz")[0] == 200, "disabled examples do not affect readiness")
    teaching = start(dict(env, ENABLE_EXAMPLE="true"))
    expect(request(teaching, "GET", "/readyz")[0] == 503, "enabled examples require their schema")
    expect(request(teaching, "GET", "/example/notes")[0] == 401, "omitted AUTH_MODE defaults to local authentication")
    sql("ALTER TABLE notes_hidden RENAME TO notes")
    sql("ALTER TABLE account_sessions RENAME TO sessions_hidden")
    expect(request(api, "GET", "/readyz")[0] == 503, "missing enabled account schema fails readiness")
    sql("ALTER TABLE sessions_hidden RENAME TO account_sessions")
    registration = {"email": " User@Example.Test ", "password": password, "display_name": "User"}
    for invalid in [{}, {"email": None, "password": 42, "display_name": []}, {"email": "bad", "password": "short", "display_name": " "}]:
        status, problem, _ = request(api, "POST", "/api/auth/register", dict(invalid, unexpected="secret-sentinel"))
        paths = [issue["path"] for issue in problem.get("issues", [])]
        expect(status == 422 and all([field] in paths for field in registration) and [] in paths, "account validation aggregates safe field and unknown-key issues")
        expect("secret-sentinel" not in json.dumps(problem), "validation never echoes unknown input")
    expect(request(api, "POST", "/api/auth/register", dict(registration, password="short"))[0] == 422, "short passwords return structured validation errors")
    expect(request(api, "POST", "/api/auth/register", dict(registration, role="admin"))[0] == 422, "client-selected permissions are rejected")
    expect(request(api, "POST", "/api/auth/register", raw='{"email":"a","email":"b"}')[0] == 422, "duplicate JSON fields are rejected")
    status, body, headers = request(api, "POST", "/api/auth/register", registration)
    expect(status == 201 and body["data"]["user"]["email"] == "user@example.test", "registration creates a normalized user and session")
    first = body["data"]["access_token"]; secrets.append(first)
    uid = body["data"]["user"]["id"]
    expect(headers.get("cache-control") == "no-store" and body["data"]["expires_in"] == 86400, "session response is noncacheable and expires in one day")
    expect(set(body["data"]["user"]) == {"id", "email", "display_name", "email_verified"} and body["data"]["user"]["email_verified"] is False, "user serialization exposes unverified status without persistence secrets")
    expect(sql("SELECT count(*) FROM account_verifications WHERE token_hash IS NULL") == "1", "registration atomically queues verification without a raw credential")
    SMTP.reject = True
    command([binary, "auth:mail-once"], env)
    expect(sql("SELECT attempts=1 AND NOT delivered FROM account_verifications") == "t", "verification delivery retries temporary SMTP failure")
    SMTP.reject = False
    sql("UPDATE account_verifications SET available_at=clock_timestamp()")
    verification = deliver("Verification")
    expect(request(api, "POST", "/api/auth/verify-email", {"token": verification})[0] == 401, "verification requires authentication")
    expect(request(api, "POST", "/api/auth/email-verification")[0] == 401, "resend requires authentication")
    expect(request(api, "POST", "/api/auth/reset-password", {"token": verification, "password": password})[0] == 400, "verification credentials cannot reset passwords")
    expect(request(api, "POST", "/api/auth/email-verification", token=first)[0] == 202, "current user can request verification again")
    expect(request(api, "POST", "/api/auth/verify-email", {"token": verification}, first)[0] == 400, "resend supersedes earlier verification")
    expired_verification = deliver("Verification")
    sql("UPDATE account_verifications SET expires_at=clock_timestamp()-interval '1 second'")
    expect(request(api, "POST", "/api/auth/verify-email", {"token": expired_verification}, first)[0] == 400, "expired verification fails")
    request(api, "POST", "/api/auth/email-verification", token=first)
    verification = deliver("Verification")
    sql("UPDATE users SET email='changed@example.test'")
    expect(request(api, "POST", "/api/auth/verify-email", {"token": verification}, first)[0] == 400, "verification is bound to the requested email")
    sql("UPDATE users SET email='user@example.test'")
    expect(sql("SELECT password_hash LIKE '$argon2id$%' FROM users") == "t", "passwords use Argon2id hashes")
    expect(sql("SELECT length(token_hash) FROM bracel_tokens LIMIT 1") == "64", "bearer secrets are stored as digests")
    expect(request(api, "POST", "/api/auth/register", registration)[0] == 409, "duplicate normalized email cannot create another user")
    with concurrent.futures.ThreadPoolExecutor(2) as pool:
        results = list(pool.map(lambda _: request(api, "POST", "/api/auth/register", dict(registration, email="race@example.test")), range(2)))
    expect(sorted(r[0] for r in results) == [201,409], "concurrent registration creates exactly one account")
    for result in results:
        if result[0] == 201:
            other = result[1]["data"]["access_token"]
            secrets.append(other)
    expect(request(api, "POST", "/api/auth/verify-email", {"token": verification}, other)[0] == 400, "another user cannot consume verification")
    with concurrent.futures.ThreadPoolExecutor(2) as pool:
        results = list(pool.map(lambda _: request(api, "POST", "/api/auth/verify-email", {"token": verification}, first), range(2)))
    expect(sorted(r[0] for r in results) == [204, 400], "verification is consumed atomically once")
    expect(request(api, "POST", "/api/auth/verify-email", {"token": verification}, first)[0] == 400, "consumed verification cannot be replayed")
    expect(request(api, "POST", "/api/auth/email-verification", token=first)[0] == 202 and sql(f"SELECT count(*) FROM account_verifications WHERE user_id='{uid}'") == "0", "verified users do not queue more verification mail")
    # Clear the other user's initial delivery with the persistent worker below.
    worker_log = (artifact / "mail-worker.log").open("w"); logs.append(worker_log)
    worker = subprocess.Popen([binary, "auth:mail-work"], env=env, stdout=worker_log, stderr=worker_log); processes.append(worker)
    for _ in range(100):
        if sql("SELECT count(*) FROM account_verifications WHERE NOT delivered") == "0": break
        time.sleep(.1)
    expect(sql("SELECT count(*) FROM account_verifications WHERE delivered") == "1", "persistent mail worker delivers verification")
    request(api, "POST", "/api/auth/email-verification", token=other)
    for _ in range(100):
        if sql("SELECT count(*) FROM account_verifications WHERE NOT delivered") == "0": break
        time.sleep(.1)
    expect(sql("SELECT count(*) FROM account_verifications WHERE delivered") == "1", "persistent mail worker delivers a second request with the same transport")
    worker.terminate(); worker.wait(timeout=20)
    for message in messages:
        content = email.message_from_bytes(message, policy=email.policy.default).get_body(preferencelist=("plain",)).get_content()
        secrets.extend(re.findall(r"token: ([a-f0-9]{64})", content))
    messages.clear()
    wrong = request(api, "POST", "/api/auth/login", {"email":"user@example.test","password":"wrong"})
    unknown = request(api, "POST", "/api/auth/login", {"email":"missing@example.test","password":"wrong"})
    expect(wrong[0] == unknown[0] == 401 and wrong[1]["detail"] == unknown[1]["detail"], "unknown accounts and incorrect passwords share a safe response")
    expect(request(api, "GET", "/api/users/me")[0] == 401, "profile requires a bearer session even when public registration is enabled")
    second = issue()
    replica = start(env)
    expect(request(replica,"GET","/api/users/me",token=first)[1]["data"]["email_verified"] is True, "verification is immediately visible across replicas using the original session")
    request(api, "POST", "/api/auth/email-verification", token=other)
    request(api, "POST", "/api/auth/email-verification", token=other)
    expect(request(replica, "POST", "/api/auth/email-verification", token=other)[0] == 429, "resend quotas are shared across replicas")
    sql("UPDATE account_verifications SET expires_at=clock_timestamp()-interval '1 second'")
    expect(request(replica,"GET","/api/users/me",token=second)[1]["data"]["id"] == uid, "sessions work across replicas")
    expect(request(api,"GET","/api/users/"+uid,token=second)[0] == 404, "no public user directory or arbitrary-user endpoint exists")
    expect(request(api,"PATCH","/api/users/me",{"display_name":"Updated"},second)[1]["data"]["display_name"] == "Updated", "current user can update their profile")
    expect(request(api,"PATCH","/api/users/me",{"display_name":"Updated","email":"other@example.test"},second)[0] == 422, "profile cannot change identity fields")
    command_result=command([binary,"tokens:issue","external",uid,"account:self","3600"],env)
    external=json.loads(command_result.splitlines()[-1])["result"]["token"];secrets.append(external)
    expect(request(api,"GET","/api/users/me",token=external)[0] == 401, "another issuer cannot impersonate a local subject")
    expect(request(api,"POST","/api/auth/email-verification",token=external)[0] == 401 and request(api,"POST","/api/auth/verify-email",{"token":verification},external)[0] == 401, "external identities cannot request or consume local verification")
    expect(request(api,"POST","/api/auth/logout",token=first)[0] == 204, "logout succeeds")
    expect(request(replica,"GET","/api/users/me",token=first)[0] == 401 and request(api,"GET","/api/users/me",token=second)[0] == 200, "logout revokes only the presented session across replicas")
    third=issue()
    expect(request(api,"POST","/api/auth/logout-all",token=second)[0] == 204, "logout-all succeeds")
    expect(request(replica,"GET","/api/users/me",token=third)[0] == 401, "logout-all revokes other sessions")
    current=issue()
    known=request(api,"POST","/api/auth/forgot-password",{"email":"user@example.test"})
    missing=request(api,"POST","/api/auth/forgot-password",{"email":"missing@example.test"})
    expect(known[:2] == missing[:2] and known[0] == 202, "forgot-password does not disclose account existence")
    expect(sql("SELECT count(*) FROM account_resets WHERE token_hash IS NULL") == "1", "queued reset intent contains no credential")
    SMTP.reject=True
    command([binary,"auth:mail-once"],env)
    expect(sql("SELECT attempts=1 AND NOT delivered FROM account_resets") == "t", "temporary SMTP failure retains retryable intent")
    SMTP.reject=False
    sql("UPDATE account_resets SET available_at=clock_timestamp()")
    reset_token=deliver()
    expect(request(api,"POST","/api/auth/verify-email",{"token":reset_token},current)[0] == 400, "reset credentials cannot verify email")
    expect(len(messages) == 1 and sql("SELECT delivered FROM account_resets") == "t", "mail worker delivers password reset through SMTP")
    expect(request(api,"POST","/api/auth/reset-password",{"token":"0"*64,"password":new_password})[0] == 400, "unknown reset token is rejected")
    with concurrent.futures.ThreadPoolExecutor(2) as pool:
        results=list(pool.map(lambda _: request(api,"POST","/api/auth/reset-password",{"token":reset_token,"password":new_password}),range(2)))
    expect(sorted(r[0] for r in results) == [204,400], "reset token is consumed atomically once under concurrency")
    expect(request(replica,"GET","/api/users/me",token=current)[0] == 401, "password reset revokes all prior sessions")
    expect(request(api,"POST","/api/auth/login",{"email":"user@example.test","password":password})[0] == 401, "previous password no longer logs in")
    current=issue(candidate=new_password)
    expect(request(api,"POST","/api/auth/reset-password",{"token":reset_token,"password":password})[0] == 400, "consumed reset cannot be replayed")
    sql("DELETE FROM account_quotas")
    request(api,"POST","/api/auth/forgot-password",{"email":"user@example.test"})
    superseded=deliver()
    request(api,"POST","/api/auth/forgot-password",{"email":"user@example.test"})
    latest=deliver()
    expect(request(api,"POST","/api/auth/reset-password",{"token":superseded,"password":password})[0] == 400, "new reset request supersedes the previous credential")
    with concurrent.futures.ThreadPoolExecutor(2) as pool:
        login_future=pool.submit(request,api,"POST","/api/auth/login",{"email":"user@example.test","password":new_password})
        reset_future=pool.submit(request,api,"POST","/api/auth/reset-password",{"token":latest,"password":password})
        racing_login,racing_reset=login_future.result(),reset_future.result()
    expect(racing_reset[0] == 204 and racing_login[0] in (200,401), "password reset and in-flight login serialize safely")
    if racing_login[0] == 200:
        raced_token=racing_login[1]["data"]["access_token"];secrets.append(raced_token)
        expect(request(replica,"GET","/api/users/me",token=raced_token)[0] == 401, "session from racing old-password login cannot survive reset")
    current=issue()
    sql("DELETE FROM account_quotas")
    request(api,"POST","/api/auth/forgot-password",{"email":"user@example.test"})
    expired=deliver(); sql("UPDATE account_resets SET expires_at=clock_timestamp()-interval '1 second'")
    expect(request(api,"POST","/api/auth/reset-password",{"token":expired,"password":password})[0] == 400, "expired reset cannot change a password")
    sql("UPDATE bracel_tokens SET expires_at=clock_timestamp()-interval '1 second'")
    expect(request(api,"GET","/api/users/me",token=current)[0] == 401, "expired bearer session fails closed")
    for _ in range(10):
        response=request(api,"POST","/api/auth/login",{"email":"quota@example.test","password":"wrong"})
    expect(response[0] == 401 and request(replica,"POST","/api/auth/login",{"email":"quota@example.test","password":"wrong"})[0] == 429, "email login quotas are shared across replicas")
    disabled=start(dict(env,ALLOW_REGISTRATION="false"))
    expect(request(disabled,"POST","/api/auth/register",registration)[0] == 403, "registration can be disabled without removing login")
    no_mail=start({k:v for k,v in env.items() if k!="MAIL_LOCAL_PORT"})
    expect(request(no_mail,"POST","/api/auth/forgot-password",{"email":"user@example.test"})[0] == 503, "missing SMTP configuration fails explicitly")
    no_mail_token = issue()
    expect(request(no_mail,"POST","/api/auth/email-verification",token=no_mail_token)[0] == 503, "verification resend without SMTP fails explicitly")
    request(api, "POST", "/api/auth/logout", token=no_mail_token)
    off=start(dict(env,AUTH_MODE="off",ENABLE_ACCOUNTS="false"))
    expect(request(off,"POST","/api/auth/login",{})[0] == 404, "account routes can be disabled")
    command([binary,"auth:cleanup"],env)
    expect(sql("SELECT count(*) FROM account_sessions") == "0" and sql("SELECT count(*) FROM account_resets") == "0", "cleanup removes expired credentials and reset intents")
    expect(sql("SELECT count(*) FROM account_verifications") == "0", "cleanup removes expired verification intents")
    report["ok"] = True
except Exception:
    report["failure"] = traceback.format_exc()
finally:
    for proc in processes: proc.terminate()
    for proc in processes:
        try: proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            proc.kill();proc.wait();report["ok"]=False
    for log in logs: log.close()
    if smtp: smtp.shutdown();smtp.server_close()
    rendered=json.dumps(transcript)
    log_text="".join(p.read_text() for p in artifact.glob("*.log"))
    if any(secret in rendered or secret in log_text for secret in secrets):
        report["ok"]=False;report["redaction_failure"]=True
    else:
        report["assertions"].append({"description":"logs and transcript contain no passwords or credential secrets","passed":True})
    if report["ok"]:
        command(["psql","-X","-v","ON_ERROR_STOP=1","-c",f"DROP SCHEMA {schema} CASCADE"],db_env)
    else: report["retained_schema"]=schema
    (artifact/"report.json").write_text(json.dumps(report,indent=2))
    (artifact/"transcript.json").write_text(rendered)
    print(json.dumps({"ok":report["ok"],"artifact":str(artifact)}))
raise SystemExit(0 if report["ok"] else 1)
