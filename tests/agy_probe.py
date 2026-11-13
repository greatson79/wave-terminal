#!/usr/bin/env python3
"""Isolated source harness: python3 tests/agy_probe.py (no daemon/app build).

Compiles the actual agy parser/collector against minimal daemon/account sinks.
Only the 300-second sleep is accelerated to 30 ms; PATH contains only fake agy.
"""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / 'src/bin/cysd/usage.rs').read_text()
start = source.index('fn parse_agy_cli_usage(')
end = source.index('\n#[cfg(test)]', start)
collector = source[start:end]
assert collector.count('Duration::from_secs(300)') == 1, 'production cadence changed'
collector = collector.replace('Duration::from_secs(300)', 'Duration::from_millis(30)')
prelude = r'''
use serde_json::Value;
use std::{sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::Duration};
use state::{Daemon, now_epoch};
#[derive(Clone)]
pub struct RateWindow { label: String, used_pct: f64, resets_at: Option<f64> }
mod state {
    use super::*;
    pub struct Surface { pub exited: AtomicBool, pub agent_meta: Mutex<Option<(String, String)>> }
    #[derive(Default)]
    pub struct Daemon {
        pub surfaces: Mutex<std::collections::HashMap<u64, Arc<Surface>>>,
        pub samples: Mutex<Vec<Vec<RateWindow>>>,
    }
    pub fn now_epoch() -> f64 { 1.0 }
    pub trait HideConsole { fn hide_console(&mut self) -> &mut Self; }
    impl HideConsole for tokio::process::Command { fn hide_console(&mut self) -> &mut Self { self } }
}
mod accounts {
    use super::*;
    pub fn note_rate(d: &Arc<Daemon>, agent: &str, _: &str, rate: &[RateWindow], src: &str, _: f64) {
        assert_eq!((agent, src), ("agy", "agy-cli"));
        assert_eq!(rate[0].label, "Gemini Models 5h");
        assert_eq!(rate[0].used_pct, 25.0);
        assert!(rate[0].resets_at.is_some());
        d.samples.lock().unwrap().push(rate.to_vec());
    }
}
'''
tests = r'''
#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    fn fixture(name: &str, account: Option<&str>, agent: Option<(&str, bool)>, reply: &str, status: u8, stderr: bool) -> (std::path::PathBuf, Arc<Daemon>) {
        let home = std::env::temp_dir().join(format!("agy-probe-{}-{name}", std::process::id()));
        fs::create_dir_all(home.join(".cys")).unwrap();
        if let Some(provider) = account {
            fs::write(home.join(".cys/accounts.json"), format!(r#"{{"accounts":[{{"provider":"{provider}"}}]}}"#)).unwrap();
        }
        fs::write(home.join("reply"), reply).unwrap();
        let redirect = if stderr { " >&2" } else { "" };
        fs::write(home.join("agy"), format!("#!/bin/sh\nprintf 'x\\n' >> \"$HOME/count\"\n/bin/cat \"$HOME/reply\"{redirect}\nexit {status}\n")).unwrap();
        fs::set_permissions(home.join("agy"), fs::Permissions::from_mode(0o700)).unwrap();
        std::env::set_var("HOME", &home);
        std::env::set_var("PATH", &home);
        let daemon = Arc::new(Daemon::default());
        if let Some((a, exited)) = agent {
            daemon.surfaces.lock().unwrap().insert(1, Arc::new(state::Surface {
                exited: AtomicBool::new(exited), agent_meta: Mutex::new(Some((a.into(), "fake".into()))),
            }));
        }
        (home, daemon)
    }
    fn count(home: &std::path::Path) -> usize { fs::read_to_string(home.join("count")).unwrap_or_default().lines().count() }
    const NORMAL: &str = r#"{"status":"SUCCESS","usage":{"total_tokens":0},"response":"Gemini Models\tFive Hour Limit Remaining\t75%\t2026-11-14T00:00:00Z"}"#;
    async fn wait_count(home: &std::path::Path, n: usize) {
        for _ in 0..400 {
            if count(home) >= n { return; }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("fake agy failed to reach {n} launches; got {}", count(home));
    }
    async fn wait_samples(d: &Arc<Daemon>, n: usize) {
        for _ in 0..400 {
            if d.samples.lock().unwrap().len() >= n { return; }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("snapshot count below {n}");
    }
    #[tokio::test]
    async fn no_configuration_never_launches() {
        let (home, d) = fixture("none", None, None, NORMAL, 0, false);
        spawn_agy_collector(d);
        tokio::time::sleep(Duration::from_millis(1000)).await;
        assert_eq!(count(&home), 0, "unconfigured collector launched agy");
    }
    #[tokio::test]
    async fn auth_failure_stops_for_daemon_lifetime() {
        for (i, reply, status, stderr) in [
            (0, "not logged in", 0, false),
            (1, r#"{"status":"ERROR","error":"Authentication failed"}"#, 1, false),
            (2, "Unauthorized (401)", 1, true),
            (3, "invalid_grant: token expired", 1, true),
        ] {
            let (home, d) = fixture(&format!("auth-{i}"), Some("agy"), None, reply, status, stderr);
            spawn_agy_collector(d.clone());
            wait_count(&home, 1).await;
            tokio::time::sleep(Duration::from_millis(1000)).await;
            assert_eq!(count(&home), 1, "collector retried after auth failure: {reply}");
            assert!(d.samples.lock().unwrap().is_empty());
        }
    }
    #[tokio::test]
    async fn configured_account_keeps_collecting() {
        let (home, d) = fixture("account", Some("agy"), None, NORMAL, 0, false);
        spawn_agy_collector(d.clone());
        wait_count(&home, 3).await;
        wait_samples(&d, 3).await;
    }
    #[tokio::test]
    async fn healthy_usage_with_auth_phrase_keeps_collecting() {
        let mut payload: Value = serde_json::from_str(NORMAL).unwrap();
        payload["usage"]["note"] = Value::String("unauthorized".into());
        let (home, d) = fixture("healthy-auth-phrase", Some("agy"), None, &payload.to_string(), 0, false);
        spawn_agy_collector(d.clone());
        wait_samples(&d, 3).await;
        assert!(count(&home) >= 3);
    }
    #[tokio::test]
    async fn live_reviewer_seat_keeps_collecting() {
        let (home, d) = fixture("reviewer", None, Some(("gemini", false)), NORMAL, 0, false);
        spawn_agy_collector(d.clone());
        wait_count(&home, 3).await;
        wait_samples(&d, 3).await;
        d.surfaces.lock().unwrap().get(&1).unwrap().exited.store(true, Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(20)).await;
        let before = count(&home);
        tokio::time::sleep(Duration::from_millis(1000)).await;
        assert_eq!(count(&home), before, "exited seat still authorizes probing");
    }
    #[tokio::test]
    async fn configuration_is_rechecked_on_ticks() {
        let (home, d) = fixture("late", Some("codex"), Some(("agy", true)), NORMAL, 0, false);
        spawn_agy_collector(d.clone());
        tokio::time::sleep(Duration::from_millis(1000)).await;
        assert_eq!(count(&home), 0);
        d.surfaces.lock().unwrap().get(&1).unwrap().exited.store(false, Ordering::Relaxed);
        wait_count(&home, 2).await;
    }
}
'''
with tempfile.TemporaryDirectory(prefix='agy-probe-harness-') as temp:
    crate = Path(temp)
    (crate / 'src').mkdir()
    (crate / 'Cargo.toml').write_text('''[package]
name = "agy-probe-harness"
version = "0.0.0"
edition = "2021"
[dependencies]
tokio = { version = "1", features = ["rt", "macros", "time", "process", "sync"] }
serde_json = "1"
chrono = "0.4"
dirs = "6"
''')
    (crate / 'src/lib.rs').write_text(prelude + collector + tests)
    env = dict(os.environ, CARGO_TARGET_DIR=str(crate / 'target'), TMPDIR=str(crate))
    failures = 0
    for name in ['no_configuration_never_launches', 'auth_failure_stops_for_daemon_lifetime', 'configured_account_keeps_collecting', 'healthy_usage_with_auth_phrase_keeps_collecting', 'live_reviewer_seat_keeps_collecting', 'configuration_is_rechecked_on_ticks']:
        result = subprocess.run(['cargo', 'test', '--offline', '--manifest-path', str(crate / 'Cargo.toml'), name, '--', '--test-threads=1', '--nocapture'], env=env)
        failures += result.returncode != 0
    raise SystemExit(bool(failures))
