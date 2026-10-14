#!/usr/bin/env python3
"""Local-only sidebar help and pane input gate; saves six PNG screenshots."""

import http.server
import json
import socket
import threading
from pathlib import Path

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
OUTPUT = Path("<local-home>/Desktop/Ai_works/<local-evidence-dir>")

SHIM = r"""
localStorage.setItem('cys-wsbar-w', '400');
window.__calls = [];
window.__TAURI__ = {
  core: { invoke: async (command, args = {}) => {
    window.__calls.push({command, args});
    if (command === 'daemon_status') return {daemon_pid: 123, socket_path: 'mock'};
    if (command === 'list_depts') return {depts: {}};
    if (command === 'dept_tombstones') return [];
    if (command === 'ceo_pending') return false;
    if (command === 'create_surface') return {surface_id: 7};
    if (command === 'attach_surface') return {output_event: 'mock-output', exited_event: 'mock-exit'};
    if (command === 'list_surfaces') return {surfaces: [{surface_id: 7, title: 'surface 7', role: null, live_cwd: null, exited: false, usage: null}]};
    if (command === 'org_status') return {surfaces: [{surface_id: 7, role: null, idle_secs: 0, agent_alive: true, status: {state: 'working', task: '홈페이지 글 고치는 중', context_pct: null}}], feed: {pending: 0}};
    if (command === 'feed_list') return {items: []};
    if (command === 'help_username') return 'alice';
    if (command === 'prepare_help') return {install_id: '0123456789abcdef0123456789abcdef', installer_version: 'app-0.2.0', os: 'mac', step: '0/0', notice_shown: true, code: args.code, message: args.message};
    if (command === 'send_help') {
      const response = await fetch('/api/help', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(args.payload)});
      if (!response.ok) throw `전송 실패 (${response.status})`;
      return {id: (await response.json()).id.slice(0, 8)};
    }
    return {};
  }},
  event: {listen: async () => () => {}}
};
"""


def main():
    assert (DIST / "index.html").exists(), "run ui/build.sh first"
    OUTPUT.mkdir(parents=True, exist_ok=True)
    received = []
    state = {"status": 201}

    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=str(DIST), **kwargs)

        def log_message(self, *_args):
            pass

        def do_POST(self):
            assert self.path == "/api/help"
            received.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            status = state["status"]
            body = json.dumps({"id": "abcdefgh1234567890abcdef12345678"} if status == 201 else {"error": "busy"}).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    server = http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    origin = f"http://127.0.0.1:{port}"
    try:
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch()
            context = browser.new_context(viewport={"width": 1440, "height": 900}, permissions=["clipboard-read", "clipboard-write"])
            page = context.new_page()
            page.add_init_script(SHIM)
            blocked = []

            def route_local(route):
                if route.request.url.startswith(origin + "/"):
                    route.continue_()
                else:
                    blocked.append(route.request.url)
                    route.abort()

            page.route("**/*", route_local)
            page.goto(origin + "/index.html")
            page.locator(".pane-title-text").get_by_text("1번 · 홈페이지 글 고치는 중").wait_for(timeout=10000)
            assert not blocked, f"outside request attempted: {blocked}"
            assert not page.locator("#btn-master-start, #btn-dept-master, #btn-ws-dept").count()

            page.screenshot(path=str(OUTPUT / "01_버튼없는_사이드바.png"))
            assert page.locator("#help-message").input_value() == ""
            page.screenshot(path=str(OUTPUT / "02_빈입력창.png"))
            page.locator(".pane-title-text").screenshot(path=str(OUTPUT / "06_패인번호_작업내용.png"))

            network_after_load = []
            page.on("request", lambda request: network_after_load.append(request.url))
            page.evaluate("navigator.clipboard.writeText('ls\\npwd\\n')")
            area = page.locator("#help-message")
            area.click()
            page.keyboard.press("ControlOrMeta+V")
            assert "ls\npwd\n" in area.input_value(), repr(area.input_value())
            area.press("Enter")
            area.press("Control+C")
            sent_to_pty = page.evaluate("window.__calls.filter(x => ['send_input','write_surface','send_text','send_key'].includes(x.command))")
            assert not sent_to_pty, f"PTY calls: {sent_to_pty}"
            assert not network_after_load, f"requests before send: {network_after_load}"
            print("PASS C6: multiline paste, Enter, Ctrl-C -> PTY calls 0; pre-send network calls 0")

            area.fill("alice의 메일 me@example.com /Users/alice/private")
            page.locator("#help-preview-button").click()
            preview = page.locator("#help-preview").inner_text()
            assert "<EMAIL>" in preview and "/Users/<USER>/private" in preview
            assert "me@example.com" not in preview and "/Users/alice" not in preview
            page.screenshot(path=str(OUTPUT / "03_미리보기.png"))
            page.locator("#help-send").click()
            page.get_by_text("접수됨 · 접수 번호 abcdefgh").wait_for()
            assert received[-1] == json.loads(preview)
            page.screenshot(path=str(OUTPUT / "04_접수번호.png"))
            print("PASS 201: preview equals received JSON, receipt abcdefgh")

            state["status"] = 503
            area.fill("실패 뒤에도 남는 글")
            page.locator("#help-preview-button").click()
            page.locator("#help-send").click()
            page.get_by_role("button", name="다시 보내기").wait_for()
            assert area.input_value() == "실패 뒤에도 남는 글"
            assert "전송 실패 (503)" in page.locator("#help-result").inner_text()
            page.screenshot(path=str(OUTPUT / "05_실패_다시보내기.png"))
            print("PASS 503: text preserved, retry visible")
            assert not blocked, f"outside request attempted: {blocked}"
            print(f"PASS screenshots: {len(list(OUTPUT.glob('0*_*.png')))} PNG; outside requests 0")
            browser.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


if __name__ == "__main__":
    main()
