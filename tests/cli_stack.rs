//! The real CLI must parse commands on the 1 MiB main stack used by Windows.
use std::process::{Command, Stdio};

#[test]
fn cli_help_and_identity_work_with_a_small_main_stack() {
    for argument in ["--help", "phoenix-identity"] {
        #[cfg(unix)]
        let mut command = {
            let mut shell = Command::new("/bin/sh");
            shell.args([
                "-c",
                "ulimit -s 1024 || exit; exec \"$1\" \"$2\"",
                "cys-stack-probe",
                env!("CARGO_BIN_EXE_cys"),
                argument,
            ]);
            shell
        };
        #[cfg(not(unix))]
        let mut command = {
            let mut cli = Command::new(env!("CARGO_BIN_EXE_cys"));
            cli.arg(argument);
            cli
        };
        let output = command
            .env("CYS_HOME", std::env::temp_dir())
            .env("CYS_NO_AUTOSTART", "1")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{argument}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty(), "{argument} produced no output");
    }
}
