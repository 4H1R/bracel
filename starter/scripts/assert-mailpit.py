"""Assert delivery in the exercise's isolated local capture service."""
import json
import html
import urllib.request

BASE = "http://127.0.0.1:18025/api/v1"


def get(path):
    with urllib.request.urlopen(BASE + path, timeout=5) as response:
        return json.load(response)


messages = get("/messages")
assert messages["total"] == 1, messages
message = get("/message/" + messages["messages"][0]["ID"])
assert message["Subject"] == "Your note"
assert message["To"][0]["Address"] == "reader@example.test"
assert "<b>trial</b> & café" in message["Text"]
assert "<b>trial</b> & café" in html.unescape(message["HTML"])
assert "<b>trial</b>" not in message["HTML"]
print("Mailpit delivery verified: recipient, subject, plain text, escaped HTML.")
