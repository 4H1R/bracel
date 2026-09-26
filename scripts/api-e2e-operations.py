"""Middleware and tracing through actual HTTP peers."""
import gzip
traces=[]
class Collector(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body=self.rfile.read(int(self.headers.get("Content-Length","0")));traces.append(body)
        self.send_response(200);self.send_header("Content-Type","application/x-protobuf");self.send_header("Content-Length","0");self.end_headers()
    def log_message(self,*args):pass
collector_port=peer(http.server.ThreadingHTTPServer,Collector)
traced=start(dict(env,OTLP_ENDPOINT=f"http://127.0.0.1:{collector_port}/v1/traces"))
trace_id="1234567890abcdef1234567890abcdef"
status,_,_=request(traced,"GET","/api/projects",alice,headers={"traceparent":f"00-{trace_id}-1234567890abcdef-01"})
expect(status==200,"traced request succeeds")
processes[-1].terminate();processes[-1].wait(timeout=15)
expect(any(bytes.fromhex(trace_id) in body or trace_id.encode() in body for body in traces),"OTLP export preserves incoming W3C trace context")
transcript.append({"trace_batches":[{"bytes":len(body),"sha256":hashlib.sha256(body).hexdigest()} for body in traces]})
untrusted=start(dict(env,RATE_ANONYMOUS_PER_MINUTE="1"))
status,_,_=request(untrusted,"GET","/api/projects",alice,headers={"X-Forwarded-For":"192.0.2.1"})
expect(status==200,"first untrusted-proxy request succeeds")
status,_,_=request(untrusted,"GET","/api/projects",alice,headers={"X-Forwarded-For":"192.0.2.2"})
expect(status==429,"spoofed forwarded addresses cannot bypass peer quota")
trusted=start(dict(env,RATE_ANONYMOUS_PER_MINUTE="1",TRUSTED_PROXY_CIDRS="127.0.0.1/32"))
for address in ("192.0.2.1","192.0.2.2"):
    status,_,_=request(trusted,"GET","/api/projects",alice,headers={"X-Forwarded-For":address})
    expect(status==200,"explicitly trusted proxy supplies distinct client quotas")
compressed=start(dict(env,HTTP_COMPRESSION="true"))
status,_,_=request(compressed,"POST","/api/projects",alice,{"name":"Compression","description":"data "*100},{"Idempotency-Key":"compression"})
expect(status==201,"compression fixture created")
c=http.client.HTTPConnection("127.0.0.1",compressed,timeout=8)
c.request("GET","/api/projects",headers={"Authorization":"Bearer "+alice,"Accept-Encoding":"gzip"})
r=c.getresponse();body=r.read()
expect(r.getheader("Content-Encoding")=="gzip" and json.loads(gzip.decompress(body))["data"],"configured JSON compression produces valid gzip")
c.close()
c=http.client.HTTPConnection("127.0.0.1",compressed,timeout=8)
c.request("GET","/api/events?topic=projects",headers={"Authorization":"Bearer "+alice,"Accept-Encoding":"gzip"})
r=c.getresponse()
expect(r.status==200 and r.getheader("Content-Encoding") is None,"SSE remains uncompressed for prompt flushing")
c.close()
