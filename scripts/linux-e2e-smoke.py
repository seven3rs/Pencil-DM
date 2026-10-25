#!/usr/bin/env python3
"""Linux Tauri WebDriver smoke test; requires tauri-driver and WebKitGTK WebDriver."""
import http.server
import json
import os
import pathlib
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

APP = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/pencil-download-manager").resolve()
DRIVER_PORT = 4444
PAYLOAD = b"pencil-download-manager-e2e-fixture-v1\n"

class QuietHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != "/fixture.txt":
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(PAYLOAD)))
        self.end_headers()
        self.wfile.write(PAYLOAD)
    def log_message(self, *_):
        pass

def request(method, path, payload=None, timeout=3):
    data = None if payload is None else json.dumps(payload).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{DRIVER_PORT}{path}", data=data, method=method,
        headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            return json.loads(response.read() or b"{}")
    except urllib.error.HTTPError as error:
        # WebDriver's 500 response usually contains the useful launch failure;
        # preserve it instead of reporting only the status code.
        detail = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"WebDriver {method} {path} failed ({error.code}): {detail}") from error

def wait_for(predicate, description, timeout=90):
    end = time.monotonic() + timeout
    last = None
    while time.monotonic() < end:
        try:
            value = predicate()
            if value:
                return value
        except Exception as exc:  # driver/app may still be starting
            last = exc
        time.sleep(.25)
    raise RuntimeError(f"timed out waiting for {description}: {last}")

def dump_diagnostics(base, driver_log):
    """Best-effort failure diagnostics. Raising the original timeout is more
    useful than losing it to a secondary error here, so every call is
    independently guarded. This exists because repeatedly guessing at
    startup-timing fixes without visibility into what the window actually
    showed wastes CI runs; print the page itself instead."""
    print("--- diagnostics: begin ---", file=sys.stderr)
    try:
        title = request("GET", base + "/title").get("value")
        print(f"window title: {title!r}", file=sys.stderr)
    except Exception as exc:
        print(f"could not fetch window title: {exc}", file=sys.stderr)
    try:
        source = request("GET", base + "/source").get("value", "")
        print(f"page source ({len(source)} chars, showing up to 4000):", file=sys.stderr)
        print(source[:4000], file=sys.stderr)
    except Exception as exc:
        print(f"could not fetch page source: {exc}", file=sys.stderr)
    try:
        png_b64 = request("GET", base + "/screenshot", timeout=15).get("value")
        if png_b64:
            import base64
            shot_path = pathlib.Path(tempfile.gettempdir()) / "pdm-e2e-failure.png"
            shot_path.write_bytes(base64.b64decode(png_b64))
            print(f"screenshot saved to {shot_path}", file=sys.stderr)
    except Exception as exc:
        print(f"could not capture screenshot: {exc}", file=sys.stderr)
    try:
        driver_log.flush()
        with open(driver_log.name, "rb") as fh:
            tail = fh.read().decode("utf-8", errors="replace")[-4000:]
        print("tauri-driver/app stdout+stderr (last 4000 chars):", file=sys.stderr)
        print(tail, file=sys.stderr)
    except Exception as exc:
        print(f"could not read tauri-driver log: {exc}", file=sys.stderr)
    print("--- diagnostics: end ---", file=sys.stderr)

def main():
    if not APP.is_file():
        raise RuntimeError(f"application binary not found: {APP}")
    with tempfile.TemporaryDirectory(prefix="pdm-e2e-") as temporary:
        home = pathlib.Path(temporary) / "home"
        downloads = home / "Downloads"
        downloads.mkdir(parents=True)
        (home / ".config").mkdir()
        (home / ".config" / "user-dirs.dirs").write_text(f'XDG_DOWNLOAD_DIR="$HOME/Downloads"\n')
        os.environ["HOME"] = str(home)
        os.environ["XDG_CONFIG_HOME"] = str(home / ".config")
        os.environ["XDG_DATA_HOME"] = str(home / ".local" / "share")
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), QuietHandler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        driver = None
        session = None
        # tauri-driver spawns the app itself, so its stdout/stderr is the
        # only place the app's own logs/panics show up. Previously discarded
        # entirely; capture to a file so dump_diagnostics() can show it on
        # failure instead of us flying blind.
        driver_log = open(pathlib.Path(temporary) / "tauri-driver.log", "w+")
        try:
            driver = subprocess.Popen(
                ["tauri-driver", "--port", str(DRIVER_PORT)],
                stdout=driver_log,
                stderr=subprocess.STDOUT,
            )
            wait_for(
                lambda: socket.create_connection(("127.0.0.1", DRIVER_PORT), timeout=.2).close() is None,
                "tauri-driver",
                timeout=20,
            )
            # WebKitGTK's WebDriver backend only allows one session-creation
            # request in flight at a time. App startup (first paint under
            # Xvfb) can comfortably exceed a few seconds on a loaded CI
            # runner, so retrying this POST through wait_for()'s short
            # per-attempt timeout would let one retry fire while the prior
            # request is still being processed server-side, which webkit2gtk
            # then rejects with "Maximum number of active sessions". Use a
            # single request with a generous timeout instead of retrying.
            created = request("POST", "/session", {"capabilities": {"alwaysMatch": {
                "browserName": "wry", "tauri:options": {"application": str(APP)}}}}, timeout=90)
            session = created.get("value", {}).get("sessionId") or created.get("sessionId")
            if not session:
                raise RuntimeError(f"WebDriver did not return a session ID: {created}")
            base = f"/session/{session}"
            def find(css):
                return request("POST", base + "/element", {"using": "css selector", "value": css})["value"]["element-6066-11e4-a52e-4f735466cecf"]
            def click(element):
                request("POST", base + f"/element/{element}/click", {})
            def send_keys(element, value):
                request("POST", base + f"/element/{element}/value", {"text": value, "value": list(value)})
            try:
                wait_for(lambda: find('[role="status"]') if request("GET", base + "/url") else False, "app ready status element")
                wait_for(lambda: "Ready" in request("GET", base + "/element/" + find('[role="status"]') + "/text").get("value", ""), "download engine ready")
                click(find('button[title="Add URL (Ctrl/Cmd+N)"]'))
                textarea = wait_for(lambda: find('textarea[placeholder="https://example.com/file.zip"]'), "add dialog")
                url = f"http://127.0.0.1:{server.server_address[1]}/fixture.txt"
                send_keys(textarea, url)
                click(find('dialog button[type="submit"]'))
                output = downloads / "fixture.txt"
                wait_for(lambda: output.is_file() and output.stat().st_size == len(PAYLOAD), "download completion", timeout=60)
                if output.read_bytes() != PAYLOAD:
                    raise RuntimeError("downloaded file content differs from fixture")
                print("E2E PASS: app became ready, initiated a local HTTP download, and verified exact file contents")
            except Exception:
                dump_diagnostics(base, driver_log)
                raise
        finally:
            if session:
                try: request("DELETE", f"/session/{session}")
                except Exception: pass
            if driver is not None:
                driver.terminate()
                try:
                    driver.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    driver.kill()
                    driver.wait()
            driver_log.close()
            server.shutdown()
            server.server_close()

if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"E2E BLOCKED/FAILED: {error}", file=sys.stderr)
        sys.exit(1)
