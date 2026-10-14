#!/usr/bin/env python3
"""Local-only sidebar help and pane input gate; saves PNG screenshots."""

import argparse
import http.server
import json
import os
import socket
import tempfile
import threading
import time
from pathlib import Path

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"

SHIM = r"""
localStorage.setItem('cys-wsbar-w', '400');
window.__calls = [];
window.__mockSurfaces = JSON.parse(localStorage.getItem('mock-surfaces') || '[]');
window.__nextSurface = Math.max(6, ...window.__mockSurfaces.map(s => s.surface_id)) + 1;
window.__TAURI__ = {
  core: { invoke: async (command, args = {}) => {
    window.__calls.push({command, args});
    if (command === 'daemon_status') return {daemon_pid: 123, socket_path: 'mock'};
    if (command === 'list_depts') return {depts: {}};
    if (command === 'dept_tombstones') return [];
    if (command === 'ceo_pending') return false;
    if (command === 'create_surface') {
      const surface_id = window.__nextSurface++;
      window.__mockSurfaces.push({surface_id, title: `surface ${surface_id}`, role: null, live_cwd: null, exited: false, usage: null});
      localStorage.setItem('mock-surfaces', JSON.stringify(window.__mockSurfaces));
      return {surface_id};
    }
    if (command === 'close_surface') {
      window.__mockSurfaces = window.__mockSurfaces.filter(s => s.surface_id !== args.surfaceId);
      localStorage.setItem('mock-surfaces', JSON.stringify(window.__mockSurfaces));
      return {};
    }
    if (command === 'attach_surface') return {output_event: 'mock-output', exited_event: 'mock-exit'};
    if (command === 'list_surfaces') return {surfaces: window.__mockSurfaces};
    if (command === 'org_status') return {surfaces: [{surface_id: 7, role: null, idle_secs: 0, agent_alive: true, status: {state: 'working', task: '홈페이지 글 고치는 중', context_pct: null}}], feed: {pending: 0}};
    if (command === 'feed_list') return {items: []};
    if (command === 'help_username') return 'alice';
    if (command === 'prepare_help') return {install_id: '0123456789abcdef0123456789abcdef', installer_version: 'app-0.2.0', os: 'mac', step: '0/0', notice_shown: true, code: 'J-APP-00', kind: args.kind, source: 'app', message: args.message};
    if (command === 'send_help') {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 250);
      try {
        const response = await fetch('/api/help', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(args.payload), signal: controller.signal});
        if (!response.ok) {
          const body = await response.json();
          if (response.status === 404 && body.error === 'not_configured') throw '아직 접수 준비 중입니다';
          throw `전송 실패 (${response.status})`;
        }
        return {id: (await response.json()).id.slice(0, 8)};
      } catch (error) {
        if (error?.name === 'AbortError') throw '연결 시간이 지났습니다';
        if (error instanceof TypeError) throw '연결할 수 없습니다';
        throw error;
      } finally { clearTimeout(timer); }
    }
    return {};
  }},
  event: {listen: async () => () => {}}
};
"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    output = args.output or Path(os.environ.get("SIDEBAR_HELP_PNG_DIR") or tempfile.mkdtemp(prefix="sidebar-help-r2-"))
    assert (DIST / "index.html").exists(), "run ui/build.sh first"
    output.mkdir(parents=True, exist_ok=True)
    received = []
    state = {"mode": "201"}

    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=str(DIST), **kwargs)

        def log_message(self, *_args):
            pass

        def do_POST(self):
            assert self.path == "/api/help"
            received.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            mode = state["mode"]
            if mode == "refused":
                self.connection.shutdown(socket.SHUT_RDWR)
                self.connection.close()
                return
            if mode == "timeout":
                time.sleep(1)
                return
            status = int(mode)
            body = json.dumps({"id": "abcdefgh1234567890abcdef12345678"} if status == 201 else {"error": "not_configured" if status == 404 else "busy"}).encode()
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
            page.locator(".pane-title-text").get_by_text("7번 · 홈페이지 글 고치는 중").wait_for(timeout=10000)
            assert not blocked, f"outside request attempted: {blocked}"
            assert not page.locator("#btn-master-start, #btn-dept-master, #btn-ws-dept").count()

            page.screenshot(path=str(output / "01_버튼없는_사이드바.png"))
            assert page.locator("#help-message").input_value() == ""
            page.screenshot(path=str(output / "02_빈입력창.png"))
            page.locator(".pane-title-text").screenshot(path=str(output / "06_패인번호_작업내용.png"))
            page.locator("#btn-new").click()
            page.locator(".pane[data-sid='8'] .pane-title-text").get_by_text("8번 · 대기 중").wait_for()
            close = page.locator(".pane[data-sid='8'] .pane-close")
            close.click()
            close.click()
            page.locator(".pane[data-sid='8']").wait_for(state="detached")
            page.locator("#btn-new").click()
            page.locator(".pane[data-sid='9'] .pane-title-text").get_by_text("9번 · 대기 중").wait_for()
            page.reload()
            page.locator(".pane[data-sid='7'] .pane-title-text").get_by_text("7번 · 홈페이지 글 고치는 중").wait_for()
            page.locator(".pane[data-sid='9'] .pane-title-text").get_by_text("9번 · 대기 중").wait_for()
            print("PASS C11: two surface IDs 7/8; closed 8 then 9; restored 7/9 keep IDs")

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
            page.screenshot(path=str(output / "03_미리보기.png"))
            page.locator("#help-send").click()
            page.get_by_text("접수됨 · 접수 번호 abcdefgh").wait_for()
            assert received[-1] == json.loads(preview)
            assert received[-1]["kind"] == "help" and received[-1]["source"] == "app" and received[-1]["code"] == "J-APP-00"
            page.locator("#help-kind").select_option("feedback")
            page.locator("#help-preview-button").click()
            page.locator("#help-send").click()
            page.wait_for_function("window.__calls.filter(x => x.command === 'send_help').length === 2")
            page.get_by_text("접수됨 · 접수 번호 abcdefgh").wait_for()
            assert received[-1]["kind"] == "feedback" and received[-1]["source"] == "app" and received[-1]["code"] == "J-APP-00"
            page.screenshot(path=str(output / "04_접수번호.png"))
            print("PASS 201: preview equals received JSON, receipt abcdefgh")

            for index, (mode, expected) in enumerate([
                ("503", "전송 실패 (503)"),
                ("429", "요청이 많습니다. 잠시 뒤"),
                ("400", "보내는 내용을 확인한 뒤"),
                ("404", "접수 기능이 아직 준비 중"),
                ("refused", "연결을 확인한 뒤"),
                ("timeout", "연결을 확인한 뒤"),
            ], start=5):
                state["mode"] = mode
                message = f"{mode} 뒤에도 남는 글"
                area.fill(message)
                page.locator("#help-preview-button").click()
                preview = page.locator("#help-preview").inner_text()
                page.locator("#help-send").click()
                page.get_by_role("button", name="다시 보내기").wait_for()
                assert area.input_value() == message, mode
                assert page.locator("#help-preview").inner_text() == preview, mode
                assert not page.locator("#help-preview").is_hidden(), mode
                assert page.locator("#help-send").is_enabled(), mode
                assert expected in page.locator("#help-result").inner_text(), mode
                page.screenshot(path=str(output / f"{index:02d}_{mode}_다시보내기.png"))
                print(f"PASS {mode}: text and preview preserved; retry enabled; guidance matched")
            assert not blocked, f"outside request attempted: {blocked}"
            print(f"PASS screenshots: {len(list(output.glob('*.png')))} PNG at {output}; outside requests 0")
            browser.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


if __name__ == "__main__":
    main()
