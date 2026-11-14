use std::io::Write;

use herdr_core::hc::{HcClient, HcSession};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    // The MCP server answers each request when it comes, so the output is not kept until the end.
    let (mut input, mut out, mut err) = (std::io::stdin().lock(), std::io::stdout().lock(), std::io::stderr().lock());
    // The owner's hc core, through its socket. A call waits 10 s; a send, an interrupt and a kill wait 60 s.
    let core = HcClient::new(HcSession::resolve_env(), 10, 60);
    let code = orch::cli::run(&args, &env, &mut input, &mut out, &mut err, &core);
    // A closed stdout or stderr leaves nowhere to report; the exit code still tells.
    let _ = out.flush();
    std::process::exit(code);
}
