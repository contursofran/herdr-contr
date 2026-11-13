use std::io::Write;

use herdr_core::hc::{HcClient, HcSession};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let mut out = Vec::new();
    let mut err = Vec::new();
    // The owner's hc core, through its socket. A call waits 10 s; a send, an interrupt and a kill wait 60 s.
    let core = HcClient::new(HcSession::resolve_env(), 10, 60);
    let code = orch::cli::run(&args, &env, &mut out, &mut err, &core);
    // A closed stdout or stderr leaves nowhere to report; the exit code still tells.
    let _ = std::io::stdout().write_all(&out);
    let _ = std::io::stderr().write_all(&err);
    std::process::exit(code);
}
