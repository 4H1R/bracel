"""Real-process API acceptance scenarios; retain redacted, replayable evidence."""
import argparse
import hashlib
import http.client
import http.server
import hmac
import json
import os
from pathlib import Path
import socket
import socketserver
import subprocess
import time
import threading
import traceback
import urllib.parse
import uuid

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("--scenario", default="mutations")
args = parser.parse_args()
run_id = uuid.uuid4().hex
artifact = ROOT / ".scratch" / "api-e2e" / run_id
artifact.mkdir(parents=True)
database = os.environ["TEST_DATABASE_URL"]
schema = "e2e_" + run_id
binary = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "bracel-starter"
report = {"schema_version": 1, "scenario": args.scenario, "ok": False, "commands": [], "assertions": []}
transcript = []
processes = []
logs = []
peers = []
webhook_calls=[]
mail_messages=[]
webhook_secret=b"test-only-webhook-secret-32-bytes-long"

class WebhookPeer(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body=self.rfile.read(int(self.headers.get("Content-Length","0")))
        timestamp=self.headers.get("webhook-timestamp","")
        event_id=self.headers.get("webhook-id","")
        signature=hmac.new(webhook_secret,(timestamp+"."+event_id+".").encode()+body,hashlib.sha256).hexdigest()
        valid=hmac.compare_digest(signature,self.headers.get("webhook-signature",""))
        webhook_calls.append({"id":event_id,"valid":valid,"body":json.loads(body)})
        self.send_response(503 if len(webhook_calls)==1 else 204);self.end_headers()
    def log_message(self,*args): pass

class SmtpPeer(socketserver.StreamRequestHandler):
    def handle(self):
        self.wfile.write(b"220 localhost ESMTP\r\n")
        while line:=self.rfile.readline():
            command=line.upper()
            if command.startswith(b"EHLO"):self.wfile.write(b"250-localhost\r\n250 8BITMIME\r\n")
            elif command.startswith(b"DATA"):
                self.wfile.write(b"354 End with dot\r\n");lines=[]
                while (part:=self.rfile.readline()) not in (b".\r\n",b""):lines.append(part)
                mail_messages.append(b"".join(lines));self.wfile.write(b"250 accepted\r\n")
            elif command.startswith(b"QUIT"):self.wfile.write(b"221 goodbye\r\n");break
            else:self.wfile.write(b"250 OK\r\n")

def peer(server_type,handler):
    server=server_type(("127.0.0.1",0),handler)
    peers.append(server);threading.Thread(target=server.serve_forever,daemon=True).start()
    return server.server_address[1]


def command(argv, env=None):
    report["commands"].append([str(x) for x in argv if str(x) != database])
    return subprocess.run(argv, env=env, check=True, capture_output=True, text=True).stdout


def expect(condition, description):
    report["assertions"].append({"description": description, "passed": bool(condition)})
    assert condition, description


def port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def start(env):
    p = port()
    env = dict(env, BIND_ADDR=f"127.0.0.1:{p}")
    log = (artifact / f"server-{len(processes)}.log").open("w")
    logs.append(log)
    process = subprocess.Popen([binary, "serve"], env=env, stdout=log, stderr=log)
    processes.append(process)
    for _ in range(150):
        if process.poll() is not None:
            raise RuntimeError("API process exited; inspect retained server log")
        try:
            c = http.client.HTTPConnection("127.0.0.1", p, timeout=1)
            c.request("GET", "/healthz")
            if c.getresponse().status == 200:
                c.close()
                return p
        except OSError:
            pass
        time.sleep(.1)
    raise TimeoutError("API startup")


def request(p, method, path, token=None, body=None, headers=None):
    h = dict(headers or {})
    if token:
        h["Authorization"] = "Bearer " + token
    if body is not None:
        h["Content-Type"] = "application/json"
    c = http.client.HTTPConnection("127.0.0.1", p, timeout=10)
    c.request(method, path, json.dumps(body) if body is not None else None, h)
    r = c.getresponse()
    raw = r.read()
    value = json.loads(raw) if raw else None
    transcript.append({"method": method, "path": path, "status": r.status, "response": value})
    result = r.status, value, dict(r.getheaders())
    c.close()
    return result


try:
    report["revision"] = command(["git", "rev-parse", "HEAD"]).strip()
    digest = hashlib.sha256()
    for base in ("crates", "starter/src", "scripts"):
        for source in sorted((ROOT / base).rglob("*")):
            if source.is_file() and source.suffix in {".rs", ".toml", ".py", ".sh"}:
                digest.update(str(source.relative_to(ROOT)).encode())
                digest.update(source.read_bytes())
    report["source_sha256"] = digest.hexdigest()
    report["lock_sha256"]=hashlib.sha256((ROOT/"Cargo.lock").read_bytes()).hexdigest()
    report["binary_sha256"]=hashlib.sha256(binary.read_bytes()).hexdigest()
    report["rust"] = command(["rustc", "--version"]).strip()
    connection = urllib.parse.urlsplit(database)
    db_env = dict(os.environ, PGDATABASE=connection.path.lstrip('/'), PGHOST=connection.hostname,
                  PGPORT=str(connection.port or 5432), PGUSER=urllib.parse.unquote(connection.username or ''),
                  PGPASSWORD=urllib.parse.unquote(connection.password or ''))
    report["postgres"] = command(["psql", "-XAt", "-c", "SHOW server_version"], db_env).strip()
    command(["psql", "-X", "-v", "ON_ERROR_STOP=1", "-c", f"CREATE SCHEMA {schema}"], db_env)
    parts = urllib.parse.urlsplit(database)
    query = urllib.parse.parse_qsl(parts.query)
    query.append(("options", f"-csearch_path={schema}"))
    scoped = urllib.parse.urlunsplit(parts._replace(query=urllib.parse.urlencode(query)))
    env = dict(os.environ, DATABASE_URL=scoped, AUTH_MODE="bearer", AUTH_MACHINE_TOKENS="true",
               AUTH_ISSUER="e2e", AUTH_AUDIENCE="e2e", ENABLE_BATTERIES="true", FILES_ROOT=str(artifact / "storage"),
               AUTH_PUBLIC_KEY_PEM=(ROOT / "starter/tests/fixtures/test-only-public.pem").read_text(),
               RATE_ANONYMOUS_PER_MINUTE="10000", RATE_AUTHENTICATED_PER_MINUTE="10000", RATE_WRITES_PER_MINUTE="10000")
    if args.scenario=="outbound":
        env.update(WEBHOOK_URL=f"http://127.0.0.1:{peer(http.server.ThreadingHTTPServer,WebhookPeer)}/events",WEBHOOK_SECRET=webhook_secret.decode(),WEBHOOK_INCOMING_SECRET=webhook_secret.decode(),MAIL_LOCAL_PORT=str(peer(socketserver.ThreadingTCPServer,SmtpPeer)),NOTIFY_EMAIL="owner@example.test")
    command([binary, "migrate"], env)

    def token(subject):
        # Token issuance stdout is intentionally excluded from artifacts.
        output = subprocess.check_output([binary, "tokens:issue", "e2e", subject, "projects:read projects:write events:read files:read files:write notifications:read notifications:write", "3600"], env=env, text=True)
        return json.loads(output.splitlines()[-1])["result"]["token"]

    alice, bob = token("alice"), token("bob")
    api = start(env)
    if args.scenario == "outbound":
        status,_,_=request(api,"POST","/api/projects",alice,{"name":"Delivery"},{"Idempotency-Key":"outbound-create"})
        expect(status==201,"delivery intents commit with resource")
        command([binary,"delivery:once","mail"],env)
        expect(len(mail_messages)==1 and b"Project created" in mail_messages[0],"queued email reaches local SMTP peer")
        command([binary,"delivery:once","webhooks"],env)
        expect(len(webhook_calls)==1 and webhook_calls[0]["valid"],"outbound webhook is signed correctly")
        for _ in range(12):
            time.sleep(1);command([binary,"delivery:once","webhooks"],env)
            if len(webhook_calls)>1:break
        expect(len(webhook_calls)==2 and webhook_calls[0]["id"]==webhook_calls[1]["id"] and all(c["valid"] for c in webhook_calls),"temporary webhook failure retries with stable delivery ID")
        content={"event":"fixture"}; raw=json.dumps(content).encode();timestamp=str(int(time.time()));event_id="incoming-fixture"
        signature=hmac.new(webhook_secret,(timestamp+"."+event_id+".").encode()+raw,hashlib.sha256).hexdigest()
        headers={"webhook-id":event_id,"webhook-timestamp":timestamp,"webhook-signature":signature}
        status,result,_=request(api,"POST","/api/webhooks/incoming",body=content,headers=headers)
        expect(status==200 and result["data"]["accepted"],"valid incoming signature persists intent")
        status,result,_=request(api,"POST","/api/webhooks/incoming",body=content,headers=headers)
        expect(status==200 and not result["data"]["accepted"],"duplicate incoming webhook is acknowledged without another intent")
        status,_,_=request(api,"POST","/api/webhooks/incoming",body={"event":"tampered"},headers=headers)
        expect(status==401,"tampered webhook is rejected")
        transcript.append({"webhooks":webhook_calls,"smtp_accepted":len(mail_messages)})
    elif args.scenario == "collections":
        ids=[]
        for n in range(3):
            status,body,_=request(api,"POST","/api/projects",alice,{"name":"Rust project "+str(n),"active":n!=1},{"Idempotency-Key":"list-create-"+str(n)})
            expect(status==201,"collection fixture is created through the API");ids.append(body["data"]["id"])
        status,first,_=request(api,"GET","/api/projects?limit=1&include=audit",alice)
        expect(status==200 and len(first["data"])==1 and len(first["data"][0]["audit"])==1,"authorized relationship is batch loaded")
        status,second,_=request(api,"GET","/api/projects?limit=1&after="+urllib.parse.quote(first["page"]["next_cursor"]),alice)
        expect(status==200 and first["data"][0]["id"]!=second["data"][0]["id"],"forward cursor visits another resource")
        status,back,_=request(api,"GET","/api/projects?limit=1&before="+urllib.parse.quote(second["page"]["previous_cursor"]),alice)
        expect(status==200 and back["data"][0]["id"]==first["data"][0]["id"],"backward cursor returns the prior resource")
        status,filtered,_=request(api,"GET","/api/projects?filter[active]=false&fields=id,name",alice)
        expect(status==200 and len(filtered["data"])==1 and set(filtered["data"][0])=={"id","name"},"boolean filtering and safe projection compose")
        status,_,_=request(api,"GET","/api/projects?fields=owner",alice)
        expect(status==400,"private fields cannot be selected")
        status,snapshot,_=request(api,"GET","/api/projects/snapshot",alice)
        expect(status==200 and len(snapshot["data"])==3 and snapshot["cursor"],"snapshot includes a consistent event cursor")
        status,_,_=request(api,"DELETE","/api/projects/"+ids[0],alice,headers={"If-Match":'"1"'})
        expect(status==204,"soft delete succeeds")
        status,_,_=request(api,"GET","/api/projects/"+ids[0],alice)
        expect(status==404,"deleted resources are hidden")
        status,_,_=request(api,"POST","/api/projects/"+ids[0]+"/restore",alice,{})
        expect(status==200,"authorized restoration succeeds")
    elif args.scenario == "files":
        payload=b"bracel attachment\n"
        status,body,_=request(api,"POST","/api/files",alice,{"name":"note.txt","size":len(payload),"sha256":hashlib.sha256(payload).hexdigest(),"content_type":"text/plain"})
        expect(status==201,"authorized upload can be initialized")
        path="/api/files/"+body["data"]["id"]
        def upload(credential,content):
            c=http.client.HTTPConnection("127.0.0.1",api,timeout=10)
            c.request("PUT",path+"/content",content,{"Authorization":"Bearer "+credential,"Content-Type":"application/octet-stream"})
            r=c.getresponse();r.read();c.close();return r.status
        expect(upload(bob,payload)==404,"other owner cannot upload to a reserved file")
        expect(upload(alice,b"wrong")==422,"checksum or size mismatch is rejected")
        expect(upload(alice,payload)==200,"matching bytes upload successfully")
        status,_,_=request(api,"POST",path+"/complete",alice,{})
        expect(status==200,"verified upload becomes available")
        c=http.client.HTTPConnection("127.0.0.1",api,timeout=10)
        c.request("GET",path+"/content",headers={"Authorization":"Bearer "+alice})
        r=c.getresponse();content=r.read();c.close()
        expect(r.status==200 and content==payload,"authorized download preserves exact bytes")
        status,_,_=request(api,"DELETE",path,alice)
        expect(status==204,"logical deletion succeeds")
        status,_,_=request(api,"GET",path+"/content",alice)
        expect(status==404,"deleted attachment cannot be downloaded")
    elif args.scenario == "delivery":
        status, created, _=request(api,"POST","/api/projects",alice,{"name":"Notify owner"},{"Idempotency-Key":"notification-create"})
        expect(status==201,"resource mutation accepts notification intent")
        status,inbox,_=request(api,"GET","/api/notifications",alice)
        expect(status==200 and len(inbox["data"])==1,"owner receives durable inbox notification")
        notification=inbox["data"][0]
        status,_,_=request(api,"POST","/api/notifications/"+notification["id"]+"/read",bob,{})
        expect(status==404,"other owner cannot mark a notification read")
        status,_,_=request(api,"POST","/api/notifications/"+notification["id"]+"/read",alice,{})
        expect(status==200,"notification can be marked read")
        status,inbox,_=request(api,"GET","/api/notifications",alice)
        expect(inbox["data"][0]["read"],"read state persists")
    elif args.scenario == "realtime":
        status, created, _ = request(api, "POST", "/api/projects", alice, {"name":"Streamed"}, {"Idempotency-Key":"stream-create"})
        expect(status == 201, "mutation commits before publication")
        def event_stream(p, credential, cursor=None):
            c = http.client.HTTPConnection("127.0.0.1", p, timeout=8)
            h={"Authorization":"Bearer "+credential}
            if cursor: h["Last-Event-ID"]=cursor
            c.request("GET", "/api/events?topic=projects", headers=h)
            r=c.getresponse()
            expect(r.status == 200, "authorized SSE subscription succeeds")
            return c,r
        def sse_event(response):
            event={}; line=bytearray()
            while True:
                byte=response.read(1)
                if not byte: raise EOFError("SSE ended before event")
                if byte != b"\n": line.extend(byte); continue
                text=line.decode().strip(); line.clear()
                if not text and event: return event
                if text.startswith("id:"): event["id"]=text[3:].strip()
                if text.startswith("data:"): event["data"]=json.loads(text[5:])
        c,r=event_stream(api,alice)
        first=sse_event(r); c.close()
        expect(first["data"]["kind"] == "project.created", "committed mutation is delivered as typed event")
        transcript.append({"event":first})
        status,_,_=request(api,"PATCH","/api/projects/"+created["data"]["id"],alice,{"name":"Updated"},{"If-Match":'"1"'})
        expect(status == 200,"second mutation committed")
        second_api=start(env)
        c,r=event_stream(second_api,alice,first["id"])
        second=sse_event(r); c.close()
        expect(second["data"]["kind"] == "project.updated" and first["id"] != second["id"], "another replica resumes after retained cursor")
        transcript.append({"event":second})
        status,_,_=request(api,"GET","/api/events?topic=projects",bob,headers={"Last-Event-ID":first["id"]})
        expect(status == 422,"another owner cannot reuse subscription cursor")
        status,_,_=request(api,"GET","/api/events?topic=forbidden",alice)
        expect(status == 403,"unregistered topics are denied")
        output=subprocess.check_output([binary,"tokens:issue","e2e","alice","events:read","60"],env=env,text=True)
        issued=json.loads(output.splitlines()[-1])["result"]
        c,r=event_stream(api,issued["token"],second["id"])
        command([binary,"tokens:revoke",issued["id"]],env)
        ended=sse_event(r)
        expect(ended.get("data")=={} and r.read(1)==b"","revocation closes an existing event stream")
        c.close()
        command([binary,"events:prune",second["id"].rsplit('.',1)[1]],env)
        status,_,_=request(api,"GET","/api/events?topic=projects",alice,headers={"Last-Event-ID":first["id"]})
        expect(status==409,"expired replay cursor explicitly requires snapshot recovery")
        status,snapshot,_=request(api,"GET","/api/projects/snapshot",alice)
        expect(status==200 and snapshot["data"][0]["name"]=="Updated","consistent snapshot recovers expired history")
        c,r=event_stream(second_api,alice,snapshot["cursor"])
        processes[1].terminate()
        expect(r.read()==b"","shutdown drains an idle stream")
        c.close()
    elif args.scenario == "mutations":
        status, body, _ = request(api, "POST", "/api/projects", alice, {"name": " ", "budget": "bad", "unknown": 1})
        expect(status == 422, "invalid input yields structured validation errors")
        expect(len(body["issues"]) >= 2, "validation aggregates failures")
        data = {"name": "Launch", "active": True, "budget": "12.50", "labels": ["rust"], "description": None}
        headers = {"Idempotency-Key": "create-launch"}
        status, body, _ = request(api, "POST", "/api/projects", alice, data, headers)
        expect(status == 201, "valid resource is created")
        project = body["data"]
        status, replay, _ = request(api, "POST", "/api/projects", alice, data, headers)
        expect(status == 201 and replay == body, "same request replays committed result")
        status, _, _ = request(api, "POST", "/api/projects", alice, dict(data, name="Changed"), headers)
        expect(status == 409, "changed request cannot reuse idempotency key")
        path = "/api/projects/" + project["id"]
        status, _, _ = request(api, "GET", path, bob)
        expect(status == 404, "another owner cannot retrieve the resource")
        status, changed, _ = request(api, "PATCH", path, alice, {"name": "Renamed"}, {"If-Match": '"1"'})
        expect(status == 200 and changed["data"]["description"] is None and changed["data"]["version"] == 2, "PATCH preserves omitted fields and advances version")
        status, _, _ = request(api, "PATCH", path, alice, {"name": "Stale"}, {"If-Match": '"1"'})
        expect(status == 412, "stale precondition cannot overwrite an edit")
        status, _, _ = request(api, "PATCH", path, alice, {"labels": [3]}, {"If-Match": '"2"'})
        expect(status == 422, "nested field types are validated")
        status, found, _ = request(api, "GET", path, alice)
        expect(status == 200 and found["data"]["name"] == "Renamed", "failed edits preserve committed state")
    else:
        exec(compile((ROOT / "scripts/api-e2e-extra.py").read_text(), "api-e2e-extra.py", "exec"))
    report["ok"] = True
except Exception:
    report["failure"] = traceback.format_exc()
finally:
    for process in processes:
        process.terminate()
    for process in processes:
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            report["ok"] = False
            report["shutdown_failure"] = True
    for log in logs:
        log.close()
    for server in peers:
        server.shutdown();server.server_close()
    # The disposable schema is retained for failed runs only.
    if report["ok"]:
        command(["psql", "-X", "-v", "ON_ERROR_STOP=1", "-c", f"DROP SCHEMA {schema} CASCADE"], db_env)
    else:
        report["retained_schema"] = schema
    (artifact / "report.json").write_text(json.dumps(report, indent=2))
    (artifact / "transcript.json").write_text(json.dumps(transcript, indent=2))
    print(json.dumps({"ok": report["ok"], "artifact": str(artifact)}))
raise SystemExit(0 if report["ok"] else 1)
