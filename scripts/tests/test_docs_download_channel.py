#!/usr/bin/env python3
"""사용자 문서의 다운로드·설치·API 경로가 우리 채널(greatson79/wave-terminal)만 가리키는지 확인.

원본(idoforgod) 링크는 「만든 사람」 출처 고지 안에서만 허용한다.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
UPSTREAM = re.compile(r'(github\.com|api\.github\.com/repos)/idoforgod\b')
OURS = 'https://github.com/greatson79/wave-terminal/releases/latest'
# 설치기 판(wave-install steps.json version) — 문서의 한 줄 설치 태그가 이 판과 같아야 한다(v0.2.4 404 사고 · G8 rc030-final 노아 자리 blocking)
INSTALLER_TAG = 'v0.3.0'
INSTALL_TAG = re.compile(r'greatson79/wave-install/releases/download/(v[^/]+)/')


def strip_credits(name, text):
    if name.endswith('.html'):
        pat = r'<section class="credits"[^>]*>.*?</section>'
    else:
        pat = r'^## 만든 사람\n.*?(?=^## |\Z)'
    out, n = re.subn(pat, '', text, flags=re.S | re.M)
    assert n == 1, f'{name}: 「만든 사람」 섹션이 정확히 1개여야 함 (발견 {n})'
    return out, re.search(pat, text, re.S | re.M).group(0)


class DocsDownloadChannel(unittest.TestCase):
    def test_no_upstream_outside_credits(self):
        for name in ('docs/index.html', 'USER-MANUAL.md'):
            text = (ROOT / name).read_text(encoding='utf-8')
            rest, credits = strip_credits(name, text)
            self.assertIsNone(UPSTREAM.search(rest), f'{name}: 출처 고지 밖 idoforgod 링크')
            self.assertIn(OURS, rest, f'{name}: releases/latest 링크 없음')
            self.assertIn('greatson79/wave-install', rest, f'{name}: 한 줄 설치 명령 없음')
            tags = set(INSTALL_TAG.findall(rest))
            self.assertEqual(tags, {INSTALLER_TAG}, f'{name}: 한 줄 설치 태그가 설치기 판과 다름 {tags}')
            self.assertIn('idoforgod/cys-terminal', credits, f'{name}: 원작 출처 누락')
            self.assertIn('MIT', credits, f'{name}: 라이선스 고지 누락')


if __name__ == '__main__':
    unittest.main()
