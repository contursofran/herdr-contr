// herdr-contr-feed: the herdr-app mod's sidecar. One command per run; the answer is JSON Lines on stdout
// (contract: docs/protocol.md). Picks the herdr-core session as the app does (HC_SESSION, HC_SOCKET).
fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if matches!(argv.first().map(String::as_str), Some("--version" | "-V")) {
        println!("herdr-contr-feed {} ({})", modfeedkit::protocol::SIDECAR_VERSION, modfeedkit::protocol::BUILD);
        std::process::exit(0);
    }
    // install is for a person: plain text and an exit code, not JSON Lines.
    if argv.first().map(String::as_str) == Some("install") {
        std::process::exit(modfeedkit::install::run(&argv[1..]));
    }
    // Claude Code's status line command: the owner's line as raw text, never an error of its own.
    if argv.first().map(String::as_str) == Some(modfeedkit::statusline::WORD) {
        use std::io::{Read, Write};
        let mut input = vec![];
        let _ = std::io::stdin().take(1 << 20).read_to_end(&mut input);
        let out = modfeedkit::statusline::wrap(&argv[1..], &input, std::time::SystemTime::now());
        let _ = std::io::stdout().write_all(&out);
        std::process::exit(0);
    }
    // `serve` keeps running (the phone page); every other command answers once and exits.
    if argv.first().map(String::as_str) == Some("serve") {
        std::process::exit(modfeedkit::serve::main(&argv[1..]));
    }
    let text = modfeedkit::cli::run(&argv);
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
    std::process::exit(0);
}
