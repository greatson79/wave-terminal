//! Portable reaper regression: real shell helper, temporary learner home, stub runtime only.
use cys::teams::TeamStore;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Fixture {
    root: PathBuf,
    home: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("teams-reaper-{}-{stamp}", std::process::id()));
        let home = root.join("Users").join("최형석");
        fs::create_dir_all(home.join("내 팀 폴더")).unwrap();
        Self { root, home }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn shell_path(path: &Path) -> String {
    path.to_str().unwrap().replace('\\', "/")
}

fn bash_path() -> PathBuf {
    #[cfg(windows)]
    {
        let git_bash =
            PathBuf::from(std::env::var_os("ProgramFiles").expect("Windows ProgramFiles"))
                .join("Git/bin/bash.exe");
        assert!(
            git_bash.is_file(),
            "Git Bash is required; never fall back to the WSL stub"
        );
        git_bash
    }
    #[cfg(unix)]
    {
        PathBuf::from("/bin/bash")
    }
}

#[test]
fn teams_reaper_and_down_sock_preserve_korean_home_store_on_both_platforms() {
    let f = Fixture::new();
    let store = TeamStore::new(f.home.join(".cys"));
    let folder = f.home.join("내 팀 폴더");
    let team = store
        .create("기획 · 팀".into(), folder.to_str().unwrap().into())
        .unwrap();
    store.mark_starters_offered().unwrap();
    let saved_path = f.home.join(".cys/teams.json");
    let saved = fs::read(&saved_path).unwrap();
    let bin = f.home.join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    for name in ["cys", "cysd", "python3"] {
        let text = if name == "python3" {
            "#!/bin/sh\nexec \"$CA_REAPER_PYTHON\" \"$@\"\n"
        } else {
            "#!/bin/sh\nexit 1\n"
        };
        let path = bin.join(name);
        fs::write(&path, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    let python = Command::new(if cfg!(windows) { "python" } else { "python3" })
        .args(["-c", "import sys; print(sys.executable)"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(python.status.success());
    let python_path = String::from_utf8(python.stdout)
        .unwrap()
        .trim()
        .replace('\\', "/");
    let pack = f.home.join("pack");
    fs::create_dir_all(pack.join("bin")).unwrap();
    fs::write(
        pack.join("bin/javis_phoenix.py"),
        "import sys\nsys.exit(0)\n",
    )
    .unwrap();
    #[cfg(windows)]
    let socket = PathBuf::from(format!(r"\\.\pipe\cys-dept-{}", team.runtime_key));
    #[cfg(unix)]
    let socket = f
        .home
        .join(format!("cys-dept-{}/cys.sock", team.runtime_key));
    let registry = f.home.join(".cys/depts.json");
    let dept = Path::new(env!("CARGO_MANIFEST_DIR")).join("cysjavis-pack/bin/cys-dept");
    let path = std::env::join_paths(
        std::iter::once(bin.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let command = |verb: &str| {
        let mut command = Command::new(bash_path());
        command.args(["--noprofile", "--norc", "-c", r#"test "$(command -v cys)" -ef "$HOME/.local/bin/cys" && test "$(command -v cysd)" -ef "$HOME/.local/bin/cysd" && test "$(command -v python3)" -ef "$HOME/.local/bin/python3" || exit 99
exec "$BASH" --noprofile --norc "$CA_REAPER_DEPT" "$@""#, "teams-reaper", verb]);
        if verb == "down-sock" {
            command.arg(&socket);
        }
        command
            .env_clear()
            .env("HOME", shell_path(&f.home))
            .env("USERPROFILE", &f.home)
            .env("PATH", &path)
            .env("CYS_NO_AUTOSTART", "1")
            .env("CYS_DEPTS_JSON", shell_path(&registry))
            .env("CYS_PACK_DIR", shell_path(&pack))
            .env("CA_REAPER_PYTHON", &python_path)
            .env("CA_REAPER_DEPT", shell_path(&dept))
            .stdin(Stdio::null());
        for key in ["SystemRoot", "WINDIR", "COMSPEC", "PATHEXT"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
    };
    for verb in ["reap", "down-sock"] {
        fs::write(
            &registry,
            serde_json::to_vec(
                &json!({"depts": {team.runtime_key.clone(): {"socket": socket, "pack_dir": pack}}}),
            )
            .unwrap(),
        )
        .unwrap();
        let output = command(verb).output().unwrap();
        assert!(
            output.status.success(),
            "{verb}: status={} {} {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(&saved_path).unwrap(), saved);
        assert_eq!(store.list().unwrap(), vec![team.clone()]);
        assert!(store.status().unwrap().starters_offered);
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(&registry).unwrap()).unwrap();
        assert_eq!(
            state["depts"],
            json!({}),
            "real runtime-registry removal must execute"
        );
    }
}
