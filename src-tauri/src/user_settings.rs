use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

pub fn account_owners(file: &Path) -> Result<HashMap<String, String>, String> {
    let data = match std::fs::read(file) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(error.to_string()),
    };
    if data.len() > 1_048_576 { return Err("user-settings.json is too large".into()); }
    let root: Value = serde_json::from_slice(&data).map_err(|_| "user-settings.json is invalid".to_string())?;
    let Some(owners) = root.get("accountOwners") else { return Ok(HashMap::new()) };
    let owners: HashMap<String, String> = serde_json::from_value(owners.clone())
        .map_err(|_| "accountOwners must be a string map".to_string())?;
    for (key, owner) in &owners {
        validate_key(key)?;
        if owner.chars().count() > 64 || owner.chars().any(char::is_control) {
            return Err("account owner label is invalid".into());
        }
    }
    Ok(owners)
}

pub fn custom_header(file: &Path) -> Result<bool, String> {
    let data = match std::fs::read(file) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    if data.len() > 1_048_576 { return Err("user-settings.json is too large".into()); }
    let root: Value = serde_json::from_slice(&data).map_err(|_| "user-settings.json is invalid".to_string())?;
    Ok(root.get("customHeader").and_then(Value::as_bool).unwrap_or(false))
}

pub fn set_account_owner(file: &Path, key: &str, owner: &str) -> Result<HashMap<String, String>, String> {
    validate_key(key)?;
    let owner = owner.trim();
    if owner.chars().count() > 64 || owner.chars().any(char::is_control) {
        return Err("owner label must be at most 64 characters and contain no control characters".into());
    }
    if std::fs::metadata(file).map(|m| m.len() > 1_048_576).unwrap_or(false) {
        return Err("user-settings.json is too large".into());
    }
    let mut root = match std::fs::read(file) {
        Ok(data) => serde_json::from_slice::<Value>(&data).map_err(|_| "user-settings.json is invalid".to_string())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Object(Map::new()),
        Err(error) => return Err(error.to_string()),
    };
    if !root.is_object() { return Err("user-settings.json must contain an object".into()); }
    let map = root.as_object_mut().unwrap();
    if !map.get("accountOwners").is_some_and(Value::is_object) {
        if map.contains_key("accountOwners") { return Err("accountOwners must be an object".into()); }
        map.insert("accountOwners".into(), Value::Object(Map::new()));
    }
    let owners = map.get_mut("accountOwners").and_then(Value::as_object_mut).unwrap();
    if owner.is_empty() { owners.remove(key); }
    else { owners.insert(key.into(), Value::String(owner.into())); }

    let parent = file.parent().ok_or_else(|| "user settings directory is unavailable".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = file.with_extension(format!("tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec_pretty(&root).map_err(|e| e.to_string())?;
    let result = write_and_replace(&temp, file, &bytes);
    if result.is_err() { let _ = std::fs::remove_file(&temp); }
    result?;
    account_owners(file)
}

fn validate_key(key: &str) -> Result<(), String> {
    let Some((provider, account)) = key.split_once(':') else { return Err("account key is invalid".into()) };
    let valid_provider = !provider.is_empty() && provider.len() <= 32
        && provider.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-');
    if !valid_provider || account.is_empty() || account.len() > 220 || account.chars().any(char::is_control) {
        return Err("account key is invalid".into());
    }
    Ok(())
}

fn write_and_replace(temp: &Path, target: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(temp).map_err(|e| e.to_string())?;
    use std::io::Write;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(temp, target).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{account_owners, custom_header, set_account_owner};
    use std::path::PathBuf;

    fn temp_file() -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("wave-user-settings-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("user-settings.json")
    }

    #[test]
    fn account_owner_write_preserves_other_settings_and_empty_removes_alias() {
        let file = temp_file();
        std::fs::write(&file, r#"{"customHeader":true,"unrelated":{"keep":1}}"#).unwrap();
        assert_eq!(set_account_owner(&file, "claude:id-1", "Work").unwrap()["claude:id-1"], "Work");
        let root: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(root["customHeader"], true);
        assert_eq!(root["unrelated"]["keep"], 1);
        assert!(set_account_owner(&file, "claude:id-1", " ").unwrap().is_empty());
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn invalid_keys_and_owner_labels_are_rejected() {
        let file = temp_file();
        assert!(set_account_owner(&file, "bad key", "Work").is_err());
        assert!(set_account_owner(&file, "claude:id-1", &"x".repeat(65)).is_err());
        assert!(account_owners(&file).unwrap().is_empty());
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn custom_header_is_enabled_only_by_local_profile() {
        let file = temp_file();
        assert!(!custom_header(&file).unwrap());
        std::fs::write(&file, r#"{"customHeader":true}"#).unwrap();
        assert!(custom_header(&file).unwrap());
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }
}
