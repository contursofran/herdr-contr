use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = orch::cli::run(&args, &env, &mut out, &mut err, &mut orch::cli::NoCoordinator);
    // A closed stdout or stderr leaves nowhere to report; the exit code still tells.
    let _ = std::io::stdout().write_all(&out);
    let _ = std::io::stderr().write_all(&err);
    std::process::exit(code);
}
