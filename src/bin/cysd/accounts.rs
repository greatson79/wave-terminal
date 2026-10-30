//! Active account usage derived from local surface observations.

use crate::state::Daemon;
use crate::usage::RateWindow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

/// shortcut: active sessions only; add profile inventory when idle accounts need sidebar rows.
/// Active account-rate view for the sidebar. Account identifiers are kept in memory only;
/// provider profile paths and credentials never cross the RPC boundary.
pub fn account_rows(daemon: &Daemon) -> serde_json::Value {
    let surfaces = daemon.surfaces.lock().unwrap();
    let mut rows: HashMap<(String, String), (String, f64, Vec<RateWindow>)> = HashMap::new();
    for surface in surfaces.values() {
        if surface.exited.load(Ordering::Relaxed) {
            continue;
        }
        let Some(usage) = surface.observed_usage.lock().unwrap().clone() else {
            continue;
        };
        if usage.rate.is_empty() {
            continue;
        }
        let (provider, account_id, label) = match usage.agent.as_str() {
            "claude" => {
                let Some((account_id, label)) = claude_account(&usage.session_file) else {
                    continue;
                };
                ("claude".to_string(), account_id, label)
            }
            "codex" => ("codex".into(), "default".into(), "OpenAI Codex".into()),
            "gemini" | "agy" | "antigravity" => (
                "antigravity".into(), "default".into(), "Antigravity (agy)".into(),
            ),
            _ => continue,
        };
        let key = (provider, account_id);
        let candidate = (label, usage.updated_at, usage.rate);
        if rows.get(&key).is_none_or(|old| candidate.1 >= old.1) {
            rows.insert(key, candidate);
        }
    }
    let mut accounts: Vec<_> = rows.into_iter().map(|((provider, account_id), (label, updated_at, rate))| {
        serde_json::json!({ "provider": provider, "account_id": account_id,
            "label": label, "updated_at": updated_at, "rate": rate })
    }).collect();
    accounts.sort_by(|a, b| {
        (a["provider"].as_str().unwrap_or(""), a["label"].as_str().unwrap_or(""))
            .cmp(&(b["provider"].as_str().unwrap_or(""), b["label"].as_str().unwrap_or("")))
    });
    serde_json::json!({ "accounts": accounts })
}

fn claude_account(session_file: &str) -> Option<(String, String)> {
    let normalized = session_file.replace('\\', "/");
    let marker = normalized.find("/projects/")?;
    let profile = PathBuf::from(&normalized[..marker]);
    let mut identity = profile.join(".claude.json");
    if !identity.is_file() {
        let sibling = PathBuf::from(format!("{}.json", profile.display()));
        if sibling.is_file() {
            identity = sibling;
        }
    }
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(identity).ok()?).ok()?;
    let account = data.get("oauthAccount")?;
    let id = account.get("accountUuid")?.as_str()?.to_string();
    let label = account.get("emailAddress").and_then(|v| v.as_str()).unwrap_or(&id).to_string();
    Some((id, label))
}

#[cfg(test)]
mod account_tests {
    use super::claude_account;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("cys-account-test-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn claude_identity_requires_uuid_and_never_reads_credentials() {
        let root = temp_dir();
        let profile = root.join(".claude-work");
        std::fs::create_dir_all(profile.join("projects/demo")).unwrap();
        let session = profile.join("projects/demo/session.jsonl");
        std::fs::write(profile.join(".claude.json"), r#"{"oauthAccount":{"accountUuid":"id-1","emailAddress":"person@example.test"}}"#).unwrap();
        assert_eq!(claude_account(&session.to_string_lossy()), Some(("id-1".into(), "person@example.test".into())));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn claude_identity_accepts_the_default_profile_sibling_file() {
        let root = temp_dir();
        let profile = root.join(".claude");
        std::fs::create_dir_all(profile.join("projects/demo")).unwrap();
        let session = profile.join("projects/demo/session.jsonl");
        std::fs::write(root.join(".claude.json"), r#"{"oauthAccount":{"accountUuid":"id-2"}}"#).unwrap();
        assert_eq!(claude_account(&session.to_string_lossy()), Some(("id-2".into(), "id-2".into())));
        std::fs::remove_dir_all(root).unwrap();
    }
}
