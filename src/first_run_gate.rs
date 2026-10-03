//! 첫기동 관문(폴더신뢰 · Bypass Permissions 면책) 판별 — **키를 보내지 않기 위한** 술어.
//!
//! 주인님 지시(2026-10-03): 관문 창이 화면에 있으면 우리 소프트웨어는 **어떤 키도** 보내지 않는다.
//! 고르는 것은 사람이다(Wave 창). 2.1.261+ 폴더신뢰 창과 면책 창은 기본 선택이 `No, exit` 라
//! Return 한 발이 좌석을 터미널로 떨어뜨린다.
//!
//! 판별 근거는 **질문 문면만**이다(원작 first_run_gates.rs 의 needle 그대로). 선택지 라벨·확인
//! 에코(`Yes, I trust this folder` 등)는 근거가 아니다 — 구 needle `trustthisfolder` 가 그 에코에
//! 재매칭돼 2발째 Return 이 면책 창을 누른 것이 2026-07-29 킬체인이다.
//! 위젯(`Enter to confirm`) AND 는 걸지 않는다: 여기서 오탐의 귀결은 '보류(키 0)'뿐이고 가역이다.

/// (질문 문면, 관문 이름). 문면은 공백 제거본으로 비교한다(TUI 폭 접힘·박스 렌더 흡수).
const GATES: &[(&str, &str)] = &[
    ("Quick safety check", "folder-trust"),
    ("Is this a project you created or one you trust", "folder-trust"),
    ("Do you trust the files in this folder", "folder-trust"),
    ("Do you trust this folder", "folder-trust"),
    ("WARNING: Claude Code running in Bypass Permissions mode", "bypass-permissions"),
    ("In Bypass Permissions mode, Claude Code will not ask for your approval", "bypass-permissions"),
];

fn flat(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 화면에 첫기동 관문이 떠 있으면 관문 이름을 돌려준다. `Some` 이면 키 0.
pub fn identify(screen: &str) -> Option<&'static str> {
    let f = flat(screen);
    GATES.iter().find(|(q, _)| f.contains(&flat(q))).map(|(_, id)| *id)
}

/// 화면 고정본 — lib·cys·cysd 시험이 공용으로 쓴다(바이너리 시험에선 lib 이 cfg(test) 가 아니다).
#[doc(hidden)]
pub mod fixtures {
    /// 구 폴더신뢰 창(기본 포커스 = Yes).
    pub const OLD_TRUST: &str = " Do you trust the files in this folder?\n\n /work/project\n\n \
❯ 1. Yes, proceed\n   2. No, exit\n\n Enter to confirm · Esc to cancel\n";
    /// 2.1.261+ 폴더신뢰 창(기본 포커스 = No, exit).
    pub const TRUST_2_1_261: &str = " Accessing workspace:\n /work/project\n\n \
Quick safety check: Is this a project you created or one you trust? (Like your own code, a\n \
well-known open source project, or work from your team).\n\n Security guide\n\n \
❯ 1. No, exit\n   2. Yes, I trust this folder\n\n Enter to confirm · Esc to cancel\n";
    /// Bypass Permissions 면책 창(기본 포커스 = No, exit).
    pub const BYPASS: &str = " WARNING: Claude Code running in Bypass Permissions mode\n\n \
In Bypass Permissions mode, Claude Code will not ask for your approval before running\n \
potentially dangerous commands.\n\n ❯ 1. No, exit\n   2. Yes, I accept\n\n \
Enter to confirm · Esc to cancel\n";
    /// 정상 준비 화면(관문 통과 뒤 · 확인 에코 잔존).
    pub const READY: &str = " Yes, I trust this folder ✔\n\n ✻ Welcome to Claude Code!\n\n \
╭──────────────────────────────╮\n│ ❯ \n╰──────────────────────────────╯\n  ? for shortcuts\n";
}

#[cfg(test)]
mod tests {
    use super::{fixtures::*, identify, GATES};

    #[test]
    fn gate_fixtures_identified() {
        assert_eq!(identify(OLD_TRUST), Some("folder-trust"));
        assert_eq!(identify(TRUST_2_1_261), Some("folder-trust"));
        assert_eq!(identify(BYPASS), Some("bypass-permissions"));
        assert_eq!(identify(READY), None, "정상 화면·확인 에코는 관문이 아니다");
    }

    /// 선택지 라벨·확인 에코는 관문 근거가 아니다(2026-07-29 킬체인 형태).
    #[test]
    fn option_labels_alone_are_not_gates() {
        for echo in ["Yes, I trust this folder ✔", "❯ 1. No, exit", "Yes, I accept", "Yes, proceed"] {
            assert_eq!(identify(echo), None, "{echo}");
        }
        for (q, _) in GATES {
            assert!(q.split_whitespace().count() >= 3, "질문 문면만: {q}");
        }
    }

    /// 좁은 폭에서 질문이 접혀도 잡는다.
    #[test]
    fn wrapped_question_still_identified() {
        assert_eq!(identify("WARNING: Claude Code running in Bypass\nPermissions mode"), Some("bypass-permissions"));
    }
}
