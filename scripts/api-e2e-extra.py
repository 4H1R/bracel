"""Additional public-process scenarios, executed in the acceptance harness context."""
import base64
import concurrent.futures
import struct

def sql(statement):
    return command(["psql","-XAt","-v","ON_ERROR_STOP=1","-c",f"SET search_path={schema}; {statement}"],db_env).strip().splitlines()[-1]

def frame(sock):
    def take(n):
        result=b""
        while len(result)<n:
            part=sock.recv(n-len(result))
            if not part: raise EOFError("socket closed")
            result+=part
        return result
    first,second=take(2);size=second&127
    if size==126:size=struct.unpack("!H",take(2))[0]
    elif size==127:size=struct.unpack("!Q",take(8))[0]
    assert size<131072
    return first&15,take(size)

def send_frame(sock,value,opcode=1):
    payload=value if isinstance(value,bytes) else json.dumps(value).encode()
    mask=b"test";length=len(payload)
    header=bytes([128|opcode,128|length]) if length<126 else bytes([128|opcode,128|126])+struct.pack("!H",length)
    sock.sendall(header+mask+bytes(c^mask[i%4] for i,c in enumerate(payload)))

if args.scenario=="identity":
    exec(compile((ROOT/"scripts/api-e2e-identity.py").read_text(),"identity-e2e","exec"))
elif args.scenario=="middleware":
    exec(compile((ROOT/"scripts/api-e2e-operations.py").read_text(),"operations-e2e","exec"))
elif args.scenario=="concurrency":
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        results=list(pool.map(lambda _:request(api,"POST","/api/projects",alice,{"name":"Concurrent"},{"Idempotency-Key":"concurrent"}),range(8)))
    expect(all(r[0]==201 for r in results) and len({r[1]["data"]["id"] for r in results})==1,"concurrent retries commit one resource and replay one result")
    expect(sql("SELECT count(*) FROM projects")=="1","one database resource survives concurrent creates")
    faulty=start(dict(env,NOTIFY_EMAIL="invalid-address"))
    status,_,_=request(faulty,"POST","/api/projects",alice,{"name":"Rollback"},{"Idempotency-Key":"rollback"})
    expect(status==409,"invalid delivery intent fails the transaction")
    expect(sql("SELECT count(*) FROM projects")=="1" and sql("SELECT count(*) FROM bracel_audit")=="1","failed intent rolls back resource and audit")
    expect(sql("SELECT count(*) FROM bracel_events WHERE topic='projects'")=="1","rollback emits no event")
    status,_,_=request(api,"POST","/api/projects",alice,{"name":"Rollback"},{"Idempotency-Key":"rollback"})
    expect(status==201,"rolled back idempotency key may be retried")
    c=http.client.HTTPConnection("127.0.0.1",api,timeout=5)
    c.request("POST","/api/projects",'{"name":"first","name":"second"}',{"Authorization":"Bearer "+alice,"Content-Type":"application/json","Idempotency-Key":"duplicate"})
    response=c.getresponse();expect(response.status==422,"duplicate JSON keys are rejected before validation");response.read();c.close()
elif args.scenario=="websocket":
    status,result,_=request(api,"POST","/api/projects",alice,{"name":"Socket"},{"Idempotency-Key":"socket"})
    expect(status==201,"socket resource created")
    sock=socket.create_connection(("127.0.0.1",api),timeout=8)
    key=base64.b64encode(os.urandom(16)).decode()
    sock.sendall((f"GET /api/socket?topic=projects HTTP/1.1\r\nHost: 127.0.0.1:{api}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\nAuthorization: Bearer {alice}\r\n\r\n").encode())
    head=b""
    while not head.endswith(b"\r\n\r\n"):head+=sock.recv(1)
    expect(b"101 Switching Protocols" in head,"authenticated WebSocket upgrade succeeds")
    send_frame(sock,{"type":"projects.patch","id":result["data"]["id"],"version":1,"patch":{"name":"From socket"}})
    for _ in range(8):
        opcode,payload=frame(sock)
        if opcode==9:send_frame(sock,payload,10);continue
        message=json.loads(payload)
        if message.get("type")=="projects.updated":break
    expect(message.get("type")=="projects.updated","authorized socket command shares transactional PATCH behavior")
    status,result,_=request(api,"GET","/api/projects/"+result["data"]["id"],alice)
    expect(result["data"]["name"]=="From socket" and result["data"]["version"]==2,"socket mutation is visible through HTTP")
    send_frame(sock,{"type":"projects.patch","id":result["data"]["id"],"version":1,"patch":{"name":"Stale"}})
    for _ in range(8):
        opcode,payload=frame(sock)
        if opcode==9:send_frame(sock,payload,10);continue
        message=json.loads(payload)
        if message.get("type")=="error":break
    expect(message.get("type")=="error","stale socket mutation is rejected")
    send_frame(sock,b"",8);sock.close()
elif args.scenario=="jobs":
    command([binary,"schedule:calendar","finite","0 0 0 1 1 * 2099","UTC"],env)
    sql("UPDATE bracel_calendar SET expression='0 0 0 1 1 * 2020',next_due_at=clock_timestamp()-interval '1 second'")
    command([binary,"schedule:calendar-tick"],env)
    expect(sql("SELECT enabled FROM bracel_calendar WHERE name='finite'")=="f","exhausted calendar delivers final occurrence and disables itself")
    expect(sql("SELECT count(*) FROM bracel_jobs WHERE kind='example.ping'")=="1","calendar enqueues final due job")
    command([binary,"schedule:calendar-tick"],env)
    expect(sql("SELECT count(*) FROM bracel_jobs")=="1","calendar tick is repeatable")
    sql("UPDATE bracel_jobs SET queue='isolated',available_at=clock_timestamp()+interval '1 hour'")
    command([binary,"jobs:once"],env)
    expect(sql("SELECT attempts FROM bracel_jobs")=="0","default worker cannot claim a named delayed job")
    sql("UPDATE bracel_jobs SET queue='default',available_at=clock_timestamp()-interval '1 second'")
    command([binary,"jobs:once"],env)
    expect(sql("SELECT status FROM bracel_jobs")=="succeeded","due job runs after becoming eligible")
elif args.scenario=="tenants":
    tenant=str(uuid.uuid4());path=f"/api/tenants/{tenant}/projects"
    status,_,_=request(api,"GET",path,alice)
    expect(status==403,"unverified tenant IDs do not grant access")
    command([binary,"tenants:grant",tenant,"e2e","alice","reader"],env)
    status,_,_=request(api,"POST",path,alice,{"name":"Tenant"},{"Idempotency-Key":"tenant"})
    expect(status==403,"tenant reader cannot write")
    command([binary,"tenants:grant",tenant,"e2e","alice","writer"],env)
    status,created,_=request(api,"POST",path,alice,{"name":"Tenant"},{"Idempotency-Key":"tenant"})
    expect(status==201,"verified tenant writer commits a scoped resource")
    status,_,_=request(api,"GET",path,bob)
    expect(status==403,"nonmember cannot read tenant resources")
    status,_,_=request(api,"GET","/api/projects/"+created["data"]["id"],alice)
    expect(status==404,"tenant resources cannot leak through owner routes")
    command([binary,"tenants:grant",tenant,"e2e","bob","reader"],env)
    status,found,_=request(api,"GET",path,bob)
    expect(status==200 and len(found["data"])==1,"second verified member reads the shared tenant resource")
    status,_,_=request(api,"POST","/api/projects/import",alice,[{"name":"One"},{"name":"Two"}],{"Idempotency-Key":"batch"})
    expect(status==201,"bounded atomic import commits its full batch")
    status,_,_=request(api,"POST","/api/projects/import",alice,[{"name":"Valid"},{"name":""}],{"Idempotency-Key":"invalid-batch"})
    expect(status==422 and sql("SELECT count(*) FROM projects")=="3","invalid batch causes no partial imports")
elif args.scenario=="operations":
    inspector=json.loads(command([binary,"inspect","--json"],env))
    application=inspector["application"]
    expect(any(r["path"]=="/api/projects" and r["enabled"] for r in application["routes"]),"inspection contains actual optional route registrations")
    expect(application["capabilities"]["api_packages"]["compiled"] and application["capabilities"]["api_packages"]["configured"],"capability inspection distinguishes compiled and configured")
    status,_,_=request(api,"GET","/api/operations/metrics",alice)
    expect(status==403,"metrics require operator permission")
    output=subprocess.check_output([binary,"tokens:issue","e2e","operator","ops:read","60"],env=env,text=True)
    operator=json.loads(output.splitlines()[-1])["result"]["token"]
    status,metrics,_=request(api,"GET","/api/operations/metrics",operator)
    expect(status==200 and "http" in metrics["data"] and "jobs" in metrics["data"],"authorized operator sees bounded HTTP and queue metrics")
else:
    raise ValueError("Unknown extra scenario")
