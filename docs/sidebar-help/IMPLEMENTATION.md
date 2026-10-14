# 사이드바 도움 요청 · 피드백

## 입력과 수신 칸

사이드바에서 종류를 고르고 글을 적은 뒤 「보내는 내용 보기」로 실제 JSON을 확인한다. 미리 보기를 본 뒤에만 보낼 수 있다. 실패하면 글과 미리 보기가 남으며 다시 보낼 수 있다. URL은 `src-tauri/src/main.rs`의 `HELP_URL` 한 곳에 있다. 시험용 `WAVE_HELP_TEST_URL`은 127.0.0.1만 받는다. 전송은 셸 없이 시스템 curl 인자 배열과 표준 입력을 쓴다.

| 서버/시트 칸 | 앱에서 보내는 값 |
| --- | --- |
| 받은 시각 | 서버가 기입 |
| 기기 종류 | `os`: `mac` 또는 `win` |
| 판 | `installer_version`: `app-0.2.0` |
| 단계 | `step`: `0/0` |
| 진단 코드 | 도움 요청 `J-APP-01`, 피드백 `J-APP-02` |
| 오류 요지 | 빈 값 (`env_report`·`log_tail` 미전송) |
| 수강생 메시지 | 가린 `message`, 최대 500자 |
| 로그 주소 | 빈 값 |
| 접수 번호 | 서버 생성 `id` |
| 종류 | 기존 `help` |

원문 근거: `repo/api/_lib/handlers.js` 57, 97–116줄의 검사와 변환, `repo/intake/apps-script/Code.gs` 30, 41–49줄의 시트 헤더와 행 조립(둘 다 사본 밖 읽기 전용).

## 기존 버튼과 패인

`ui/index.html`에 `btn-master-start`, `btn-dept-master`, `btn-ws-dept`는 없다. `ui/src/main.ts` 4519, 4527, 4606줄의 선택적 조회는 null일 때 실행되지 않는다. `addDeptWorkspace`는 명령 팔레트(3780줄)에서도 호출되므로 남긴다. `start_master`와 `start_dept_master`는 이 UI에서 버튼 이외 호출처가 없지만 이번 변경에서 지우지 않았다.

원본 naming 근거: 운영 줄 `src/bin/cysd/state.rs` 850줄의 `naming_cap`은 좌석 제목의 정본화 1회권이고, `src/bin/cys.rs` 7008–7010줄은 `{name}`·`{label}`·`{surface}` 템플릿과 `s번호` 확장을 설명한다. 이 사본의 패인 표시는 `ui/src/main.ts`의 `org_status`에서 같은 surface 식별자와 `status.task`를 읽는다. 전역 번호는 `(socket,surface_id)` 키에 저장한 단조 증가값이어서 화면 순서나 닫기/열기로 재번호하지 않는다. 사람이 고친 작업 내용은 `localStorage`에 저장하며 좌석의 `status.task`보다 우선한다. 빈 값으로 편집하면 좌석 신고값으로 복귀한다.

## 현재 검증 범위

`ui bun test`: 변경 전 104/104, 변경 후 109/109. `cargo test --offline -p cys-app`: 변경 후 21개 테스트 통과. 가짜 127.0.0.1 수신기로 201, 503, 429, 400, 404, 연결 실패를 검증했고 201의 접수 번호 및 서버가 받은 JSON의 동일성을 확인했다. Chrome의 로컬 UI에서 가짜 Tauri 명령으로 미리 보기의 메일·홈 경로 가림, 접수 번호, 503 뒤 글 보존·다시 보내기를 눈으로 확인했다. 탭 50개 모의 화면에서 마지막 탭이 스크롤로 보임을 확인했다. 기존 `wsbar_gate.py`는 환경에 Python Playwright가 없어 실행되지 않았다.

시험 중 `bunx tsc --noEmit`을 호출해 TypeScript 7.0.2 패키지 2개가 외부에서 내려왔다. 브리프의 바깥 연결 0을 위반했다. 운영 서버·구글 쪽 요청은 보내지 않았고, 이후 컴파일과 시험은 `--offline` 또는 기존 로컬 도구만 사용했다.

미완료: 패인 실제 데몬 연동 화면 캡처, PTY 붙여넣기 바이트 계측, 화면 PNG 파일 저장, Windows 실기, 서버 실수신(의도적으로 금지), 기존 e2e 전체 실행, 구현 전 실패 출력. 이 문서는 C1–C13 완료 보고가 아니다.
