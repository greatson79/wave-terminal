//! Isolated replay through the app's vendored PTY; never starts an app or daemon.
use anyhow::{ensure, Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn cursor_queries(tail: &mut Vec<u8>, bytes: &[u8]) -> usize {
    let mut joined = std::mem::take(tail);
    joined.extend_from_slice(bytes);
    let count = joined.windows(4).filter(|part| *part == b"\x1b[6n").count();
    *tail = joined[joined.len().saturating_sub(3)..].to_vec();
    count
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4,
        "usage: scroll-history-conpty INPUT OUTPUT burst|paced"
    );
    ensure!(
        args[3] == "burst" || args[3] == "paced",
        "invalid replay mode"
    );
    let input = std::fs::canonicalize(&args[1])?;
    let pair = native_pty_system().openpty(PtySize {
        rows: 35,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut reader = pair.master.try_clone_reader()?;
    let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
    let reply = writer.clone();
    let mut output = std::fs::File::create(&args[2])?;
    let mut chunks = std::fs::File::create(format!("{}.chunks.jsonl", &args[2]))?;
    let started = Instant::now();
    // INHERIT_CURSOR requires DSR answers while CreateProcess/ConPTY is starting.
    let drain = std::thread::spawn(move || -> std::io::Result<usize> {
        let mut buffer = [0; 65536];
        let mut tail = Vec::new();
        let mut total = 0;
        loop {
            let count = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => count,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::UnexpectedEof
                    ) =>
                {
                    break
                }
                Err(error) => return Err(error),
            };
            output.write_all(&buffer[..count])?;
            writeln!(
                chunks,
                "{{\"ms\":{},\"bytes\":{count}}}",
                started.elapsed().as_millis()
            )?;
            total += count;
            for _ in 0..cursor_queries(&mut tail, &buffer[..count]) {
                let mut reply = reply.lock().unwrap();
                reply.write_all(b"\x1b[1;1R")?;
                reply.flush()?;
            }
        }
        output.flush()?;
        Ok(total)
    });
    let mut cmd = CommandBuilder::new("powershell.exe");
    // Windows CommandBuilder otherwise defaults to USERPROFILE, not the evidence cwd.
    cmd.cwd(std::env::current_dir()?);
    let replay_script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("replay.ps1");
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ]);
    cmd.arg(replay_script);
    cmd.env("SCROLL_REPLAY", input.as_os_str());
    cmd.env("SCROLL_MODE", &args[3]);
    cmd.env("SCROLL_METADATA", format!("{}.metadata.json", &args[2]));
    let mut child = pair.slave.spawn_command(cmd)?;
    println!(
        "own_child_pid={:?} mode={} size=120x35",
        child.process_id(),
        args[3]
    );
    drop(pair.slave);
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        ensure!(
            Instant::now() < deadline,
            "own replay child exceeded 60s; workflow timeout is the backstop"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    // Closing our HPCON after child exit lets the concurrent reader drain to EOF.
    drop(writer);
    drop(pair.master);
    let total = drain
        .join()
        .map_err(|_| anyhow::anyhow!("reader panicked"))?
        .context("ConPTY output reader")?;
    ensure!(status.success(), "PowerShell replay failed: {status:?}");
    ensure!(total > 0, "INDETERMINATE: ConPTY emitted zero bytes");
    println!("RESULT=completed bytes={total} child_status={status:?}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::cursor_queries;
    #[test]
    fn dsr_queries_survive_read_boundaries_without_duplicate_answers() {
        let mut tail = Vec::new();
        assert_eq!(cursor_queries(&mut tail, b"text\x1b["), 0);
        assert_eq!(cursor_queries(&mut tail, b"6nmore\x1b[6n"), 2);
        assert_eq!(cursor_queries(&mut tail, b"tail"), 0);
    }
}
