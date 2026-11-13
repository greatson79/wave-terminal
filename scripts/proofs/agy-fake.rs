//! Fake quota CLI for the installed-build proof; never performs authentication or network I/O.
use std::{
    env,
    fs::OpenOptions,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let expected = [
        "-p",
        "/usage",
        "--output-format",
        "json",
        "--print-timeout",
        "10s",
    ];
    let valid_args = args.iter().map(String::as_str).eq(expected);
    let mode = env::var("AGY_FAKE_MODE").expect("AGY_FAKE_MODE required");
    let log = env::var("AGY_FAKE_COUNTER").expect("AGY_FAKE_COUNTER required");
    let event = format!("{{\"time\":{},\"pid\":{},\"executable\":{},\"cwd\":{},\"mode\":{},\"valid_args\":{},\"argv\":[{}]}}\n",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64(), std::process::id(),
        quoted(&env::current_exe().unwrap().to_string_lossy()), quoted(&env::current_dir().unwrap().to_string_lossy()),
        quoted(&mode), valid_args, args.iter().map(|s| quoted(s)).collect::<Vec<_>>().join(","));
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .unwrap()
        .write_all(event.as_bytes())
        .unwrap();
    if !valid_args {
        eprintln!("fake agy: unexpected argv");
        std::process::exit(2);
    }
    match mode.as_str() {
        "auth" => { println!("not logged in"); std::process::exit(1); }
        "normal" => println!("{{\"status\":\"SUCCESS\",\"usage\":{{\"total_tokens\":0,\"note\":\"unauthorized is ordinary fixture text\"}},\"response\":\"Gemini Models\\tFive Hour Limit Remaining\\t75%\\t2030-01-01T00:00:00Z\"}}"),
        _ => { eprintln!("fake agy: unknown mode"); std::process::exit(2); }
    }
}
