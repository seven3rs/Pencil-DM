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

def request(method, path, payload=None):
    data = None if payload is None else json.dumps(payload).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{DRIVER_PORT}{path}", data=data, method=method,
        headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=3) as response:
        return json.loads(response.read() or b"{}")

def wait_for(predicate, description, timeout=45):
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
        try:
            driver = subprocess.Popen(
                ["tauri-driver", "--port", str(DRIVER_PORT)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            wait_for(
                lambda: socket.create_connection(("127.0.0.1", DRIVER_PORT), timeout=.2).close() is None,
                "tauri-driver",
                timeout=20,
            )
            created = wait_for(lambda: request("POST", "/session", {"capabilities": {"alwaysMatch": {
                "browserName": "wry", "tauri:options": {"application": str(APP)}}}}), "application session")
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
            server.shutdown()
            server.server_close()

if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"E2E BLOCKED/FAILED: {error}", file=sys.stderr)
        sys.exit(1)
