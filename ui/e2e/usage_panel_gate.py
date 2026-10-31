#!/usr/bin/env python3
"""Render the real bundled sidebar with fake account data at 200px."""

import argparse
import http.server
import json
import threading
import time
from pathlib import Path

from playwright.sync_api import sync_playwright
from sidebar_help_gate import SHIM


ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    now = time.time()
    future = now + 86_400
    def account(provider, account_id, label, age, rates):
        return {"provider": provider, "account_id": account_id, "label": label,
                "updated_at": now - age * 60 if age is not None else None,
                "rate": [{"label": name, "used_pct": used, "resets_at": future} for name, used in rates]}
    accounts = [
        account("antigravity", "cli", "Antigravity (agy)", 641, [
            ("Claude and GPT models 5h", 12), ("Claude and GPT models 7d", 35),
            ("Gemini 4 Argon 7d", 55), ("Gemini Models 5h", 2), ("Gemini Models 7d", 31)]),
        account("antigravity", "default", "Antigravity (agy)", None, []),
        account("claude", "one", "main@example.test", 1, [("5h", 49), ("7d", 80)]),
        account("codex", "default", "OpenAI Codex", 1, [("7d", 81)]),
        account("fable", "default", "Fable", 1, [("7d", 10), ("7d Fable", 0)]),
        account("claude", "two", "second@example.test", 1, [("5h", 20)]),
        account("claude", "three", "third@example.test", 1, [("5h", 65)]),
    ]
    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw):
            super().__init__(*a, directory=str(ROOT / "dist"), **kw)
        def log_message(self, *_):
            pass
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    origin = f"http://127.0.0.1:{server.server_port}"
    try:
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch()
            page = browser.new_page(viewport={"width": 900, "height": 900}, device_scale_factor=2)
            script = """
localStorage.setItem('mock-surfaces', JSON.stringify([
  {surface_id:1,title:'Alpha·샘플A본부·책임자·s1',role:'master',exited:false,usage:null},
  {surface_id:2,title:'Beta·샘플B본부·책임자·s2',role:'master',exited:false,usage:null}
]));
localStorage.setItem('cys-layout-v2', JSON.stringify({workspaces:[
  {id:1,name:'Alpha',tree:{type:'pane',sid:1},groupId:10},
  {id:2,name:'Beta',tree:{type:'pane',sid:2},groupId:11}
],groups:[
  {id:10,name:'Alpha',auto:true,collapsed:false,pinned:false},
  {id:11,name:'Beta',auto:true,collapsed:false,pinned:false}
],active:0,counter:3,groupCounter:12}));
""" + SHIM + "\n" + """
localStorage.setItem('cys-wsbar-w', '200');
const previousInvoke = window.__TAURI__.core.invoke;
window.__TAURI__.core.invoke = (command, args) =>
  command === 'take_update_preserved_notices' ? Promise.resolve([]) :
  command === 'naming_config' ? Promise.resolve(JSON.stringify({roles:{
    master:{name:'Alpha',label:'샘플A본부·책임자'},
    worker:{name:'Beta',label:'샘플B본부·책임자'}
  },colors:{'샘플A본부':'#1266aa','샘플B본부':'#bb3311','_neutral':'#555555'}})) :
  command === 'usage_accounts_all' ? Promise.resolve({accounts: %s}) :
  command === 'account_owners' ? Promise.resolve({'claude:one':'Main','fable:default':'Main',
    'codex:default':'Codex','antigravity:cli':'Antigravity','claude:two':'Second','claude:three':'Third'}) :
  command === 'account_order' ? Promise.resolve(['claude:one','fable:default','codex:default',
    'antigravity:cli','claude:two','claude:three']) : previousInvoke(command, args);
""" % json.dumps(accounts, ensure_ascii=False)
            page.add_init_script(script)
            page.route("**/*", lambda route: route.continue_() if route.request.url.startswith(origin + "/") else route.abort())
            page.goto(origin + "/index.html")
            panel = page.locator("#sidebar-usage")
            panel.get_by_text("Argon").first.wait_for(timeout=10_000)
            assert page.locator("#wsbar").evaluate("e => Math.round(e.getBoundingClientRect().width)") == 200
            text = panel.inner_text()
            assert [h.inner_text() for h in panel.locator("strong").all()] == [
                "Main", "Codex", "Antigravity", "Second", "Third"], text
            assert panel.locator(".usage-group").count() == 0, text
            assert [r.inner_text().split("\n")[0] for r in panel.locator(".usage-row").all()][5:10] == [
                "Gemini 5h", "Gemini 7d", "Claude·GPT 5h", "Claude·GPT 7d", "Argon 7d"]
            assert "Fable 7d" in text and "7d Fable" in text and "Fable 7d Fable" not in text
            assert "641분 전" in text
            assert panel.evaluate("e => e.scrollWidth <= e.clientWidth")
            assert all(panel.locator(".usage-row").nth(i).evaluate("e => e.scrollWidth <= e.clientWidth")
                       for i in range(panel.locator(".usage-row").count()))
            heads = page.locator("#ws-tabs .ws-group-head")
            heads.nth(1).wait_for(timeout=10_000)
            assert [h.evaluate("e => getComputedStyle(e).borderLeftColor") for h in heads.all()] == [
                "rgb(18, 102, 170)", "rgb(187, 51, 17)"]
            panel.screenshot(path=str(args.output))
            page.screenshot(path=str(args.output.with_name(args.output.stem + "_full.png")))
            page.get_by_role("button", name="정렬").click()
            page.get_by_text("사이드바 접기", exact=True).click()
            assert not page.locator("#wsbar").is_visible()
            page.get_by_role("button", name="정렬").click()
            page.get_by_text("사이드바 펼치기", exact=True).click()
            assert page.locator("#wsbar").is_visible()
            page.get_by_role("button", name="정렬").focus()
            page.keyboard.press("Enter")
            page.keyboard.press("Tab")
            page.keyboard.press("Enter")
            assert not page.locator("#wsbar").is_visible()
            browser.close()
    finally:
        server.shutdown()
    print(f"PASS: {args.output}")


if __name__ == "__main__":
    main()
