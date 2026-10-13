"""Local HTTP server with download fixtures for scripts/download-probe.ps1 (and by hand).

Every fixture has deterministic bytes; /manifest.json lists what a correct download must produce (file name, size,
SHA-256) so the probe can check the files on disk, not just the browser's log.

usage: python -I scripts/fixture-server.py [port]     (default 8765, binds 127.0.0.1 only)
"""

import hashlib
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import quote


def blob(name, size):
    """Deterministic content for a fixture: a repeating pattern seeded by its name."""
    seed = hashlib.sha256(name.encode()).digest()
    out = bytearray()
    block = seed
    while len(out) < size:
        block = hashlib.sha256(block).digest()
        out += block
    return bytes(out[:size])


UNICODE_NAME = "résumé ✓ 2026.pdf"
FIXTURES = {
    # path: (downloaded file name, size, extra headers, content type)
    "/files/plain.bin": ("plain.bin", 200_000, {}, "application/octet-stream"),
    "/files/cd": ("report final.pdf", 120_000, {"Content-Disposition": 'attachment; filename="report final.pdf"'}, "application/pdf"),
    "/files/unicode": (
        UNICODE_NAME,
        64_000,
        {"Content-Disposition": "attachment; filename=\"resume.pdf\"; filename*=UTF-8''" + quote(UNICODE_NAME)},
        "application/pdf",
    ),
    "/files/large.bin": ("large.bin", 24_000_000, {}, "application/octet-stream"),
    "/files/target.zip": ("target.zip", 300_000, {}, "application/zip"),
    "/files/setup.exe": ("setup.exe", 3_396_909, {}, "application/octet-stream"),
    "/files/a/same.txt": ("same.txt", 5_000, {"Content-Disposition": 'attachment; filename="same.txt"'}, "text/plain"),
    "/files/b/same.txt": ("same.txt", 7_000, {"Content-Disposition": 'attachment; filename="same.txt"'}, "text/plain"),
    "/files/protected.bin": ("protected.bin", 90_000, {}, "application/octet-stream"),
}
BLOB_TEXT = "Generated in the page by a script.\n" * 400
BLOB_NAME = "generated.txt"

PAGES = {
    "/links.html": """<!doctype html><title>Download links</title><h1>Download links</h1>
<ul>
<li><a id="plain" href="/files/plain.bin">plain</a>
<li><a id="cd" href="/files/cd">content-disposition</a>
<li><a id="redirect" href="/redirect">redirect</a>
<li><a id="zip" href="/files/target.zip" download>zip with download attribute</a>
</ul>""",
    "/anim.html": """<!doctype html><title>Animated page</title>
<style>body{margin:0;background:#fff;font:16px system-ui}#box{width:420px;height:320px}p{margin:16px}</style>
<canvas id="box" width="420" height="320"></canvas>
<p>This page repaints every frame, so a capture taken while a menu is open shows whether it is still running.</p>
<p><a href="/links.html">Next page</a></p>
<script>
const c = document.getElementById("box").getContext("2d"); let n = 0;
function frame() { n++; c.fillStyle = "hsl(" + ((n * 7) % 360) + " 85% 62%)"; c.fillRect(0, 0, 420, 320);
  c.fillStyle = "#000"; c.font = "48px system-ui"; c.fillText(String(n), 30, 180); requestAnimationFrame(frame); }
requestAnimationFrame(frame);
</script>""",
    "/blob.html": """<!doctype html><title>Blob download</title><h1>Blob download</h1>
<script>
const text = %s;
const url = URL.createObjectURL(new Blob([text], {type: "text/plain"}));
const a = document.createElement("a");
a.href = url; a.download = %s; document.body.appendChild(a); a.click();
document.title = "Blob download started";
</script>""" % (json.dumps(BLOB_TEXT), json.dumps(BLOB_NAME)),
}


def manifest():
    items = []
    for path, (name, size, _, _) in FIXTURES.items():
        data = blob(path, size)
        items.append({"path": path, "name": name, "size": size, "sha256": hashlib.sha256(data).hexdigest()})
    data = BLOB_TEXT.encode()
    items.append({"path": "/blob.html", "name": BLOB_NAME, "size": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    return {"items": items}


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        sys.stderr.write("fixture: %s\n" % (fmt % args))

    def send_bytes(self, status, body, content_type, headers=None, slow=False):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        if self.command == "HEAD":
            return
        if not slow:
            self.wfile.write(body)
            return
        # Large file: stream in chunks over a few seconds so progress events have something to report.
        step = 256 * 1024
        for i in range(0, len(body), step):
            self.wfile.write(body[i : i + step])
            self.wfile.flush()
            time.sleep(0.04)

    def do_HEAD(self):
        self.do_GET()

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        if path == "/manifest.json":
            return self.send_bytes(200, json.dumps(manifest()).encode(), "application/json; charset=utf-8")
        if path.startswith("/t/") and path[3:].isalnum():
            # A page whose title is its name, so the tab probe can tell tabs apart.
            name = path[3:]
            return self.send_bytes(200, ("<!doctype html><title>Tab %s</title><h1>Tab %s</h1>" % (name, name)).encode(), "text/html; charset=utf-8")
        if path in PAGES:
            return self.send_bytes(200, PAGES[path].encode(), "text/html; charset=utf-8")
        if path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "/files/target.zip")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if path == "/login":
            # Sets the session cookie the protected file needs, then sends the browser on to it.
            self.send_response(302)
            self.send_header("Set-Cookie", "pbsession=ok; Path=/; HttpOnly; SameSite=Lax")
            self.send_header("Location", "/files/protected.bin")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if path == "/files/protected.bin" and "pbsession=ok" not in (self.headers.get("Cookie") or ""):
            return self.send_bytes(403, b"no session", "text/plain")
        if path == "/files/missing.bin":
            return self.send_bytes(404, b"not found", "text/plain")
        if path == "/files/broken.bin":
            # Announces 2 MB, sends 64 KB, then drops the connection: the download must end as failed, not complete.
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Content-Length", str(2_000_000))
            self.send_header("Content-Disposition", 'attachment; filename="broken.bin"')
            self.end_headers()
            self.wfile.write(blob(path, 64_000))
            self.wfile.flush()
            time.sleep(0.5)
            self.close_connection = True
            return
        if path in FIXTURES:
            name, size, headers, ctype = FIXTURES[path]
            return self.send_bytes(200, blob(path, size), ctype, headers, slow=size > 5_000_000)
        return self.send_bytes(404, b"unknown fixture", "text/plain")


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    sys.stderr.write("fixture server on http://127.0.0.1:%d\n" % port)
    server.serve_forever()


if __name__ == "__main__":
    main()
