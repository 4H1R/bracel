"""Configured OIDC/JWKS provider over real HTTP; signed access tokens use test-only keys."""
import base64

def b64(value): return base64.urlsafe_b64encode(value).decode().rstrip("=")
private=ROOT/"starter/tests/fixtures/test-only-private.pem"
modulus=subprocess.check_output(["openssl","rsa","-in",str(private),"-noout","-modulus"],stderr=subprocess.DEVNULL).decode().strip().split("=",1)[1]
provider_state={"kid":"first","available":True}
class IdentityPeer(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if not provider_state["available"]:self.send_response(503);self.end_headers();return
        value={"issuer":issuer,"jwks_uri":issuer+"/keys"} if self.path.endswith("openid-configuration") else {"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":provider_state["kid"],"n":b64(bytes.fromhex(modulus)),"e":"AQAB"}]}
        body=json.dumps(value).encode();self.send_response(200);self.send_header("Content-Length",str(len(body)));self.end_headers();self.wfile.write(body)
    def log_message(self,*args):pass
issuer=f"http://127.0.0.1:{peer(http.server.ThreadingHTTPServer,IdentityPeer)}"
identity_env=dict(env,AUTH_ISSUER=issuer,AUTH_DISCOVERY_URL=issuer,AUTH_MACHINE_TOKENS="false")
identity_env.pop("AUTH_PUBLIC_KEY_PEM",None)
identity_api=start(identity_env)
def jwt(kid="first",**overrides):
    claims={"iss":issuer,"aud":"e2e","sub":"provider-user","scope":"projects:read events:read","exp":int(time.time())+120};claims.update(overrides)
    data=(b64(json.dumps({"alg":"RS256","typ":"at+jwt","kid":kid}).encode())+"."+b64(json.dumps(claims).encode())).encode()
    signature=subprocess.check_output(["openssl","dgst","-sha256","-sign",str(private)],input=data)
    return data.decode()+"."+b64(signature)
status,_,_=request(identity_api,"GET","/api/projects",jwt())
expect(status==200,"configured provider discovery accepts a correctly signed access token")
for invalid in (jwt(aud="wrong"),jwt(iss="wrong"),jwt(exp=1),jwt("unknown")):
    status,_,_=request(identity_api,"GET","/api/projects",invalid)
    expect(status==401,"issuer, audience, expiry and unknown key fail closed")
provider_state["kid"]="rotated"
deadline=time.monotonic()+40
while time.monotonic()<deadline:
    status,_,_=request(identity_api,"GET","/api/projects",jwt("rotated"))
    if status==200:break
    time.sleep(1)
expect(status==200,"scheduled JWKS refresh accepts rotated key identifier")
status,_,_=request(identity_api,"GET","/api/projects",jwt())
expect(status==401,"removed provider key is no longer accepted")
provider_state["available"]=False
status,_,_=request(identity_api,"GET","/api/projects",jwt("rotated"))
expect(status==200,"bounded cached keys remain usable during a short provider outage")
