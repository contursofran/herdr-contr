//! Batched reports (#213), on a fake hc core, a fake clock and a fake timer. The state folder is temporary.
mod common;

use std::cell::RefCell;
use std::fs;

use common::{call, path, Fake};
use orch::batch::{self, urgent};
use orch::held::HELD_LEAD;
use serde_json::json;

fn texts(core: &Fake) -> Vec<String> {
    core.calls().into_iter().filter(|(m, _)| m == "terminal.send").map(|(_, p)| p["text"].as_str().unwrap().to_string()).collect()
}

struct Env {
    state: tempfile::TempDir,
    cfg: String,
}

impl Env {
    fn new(minutes: Option<u64>) -> Env {
        let state = tempfile::tempdir().unwrap();
        let cfg = state.path().join("agents.json");
        if let Some(m) = minutes {
            fs::write(&cfg, json!({ "report_batch_minutes": m }).to_string()).unwrap();
        }
        Env { cfg: path(&cfg).to_string(), state }
    }
    fn get(&self, k: &str) -> String {
        match k {
            "CONTR_STATE_DIR" => path(self.state.path()).to_string(),
            "CONTR_AGENTS_CONFIG" => self.cfg.clone(),
            "HC_TERMINAL_ID" => "t7".into(),
            _ => String::new(),
        }
    }
}

#[test]
fn urgent_reports() {
    for t in ["b1: blocked, needs a key", "b1: failed, tests", "b1: error x", "b1: question for the owner", "b1: done, CI failed", "b1: done, PR #5 merged", "b1: done, PR closed"] {
        assert!(urgent(t), "{t}");
    }
    for t in ["b1: done, PR #5 open", "r1: approve, ok", "b1: done, no errors found"] {
        assert!(!urgent(t), "{t}");
    }
}

#[test]
fn reports_wait_for_the_interval_or_an_urgent_one() {
    let e = Env::new(None);
    let env = |k: &str| e.get(k);
    let core = Fake::default();
    let armed = RefCell::new(Vec::new());
    let arm = |to: &str, s: u64| armed.borrow_mut().push((to.to_string(), s));

    // The first report goes at once; the next within 10 minutes waits, and arms one timer.
    assert_eq!(batch::report(&core, &env, "t1", "b1: done, PR #5", 1000, &arm).unwrap(), "sent");
    let r = batch::report(&core, &env, "t1", "b2: done, PR #6", 1060, &arm).unwrap();
    assert!(r.starts_with("batched:") && r.ends_with("in 9 min at most"), "{r}");
    batch::report(&core, &env, "t1", "b3: done, PR #7", 1120, &arm).unwrap();
    assert_eq!(*armed.borrow(), [("t1".to_string(), 540)]);
    assert_eq!(texts(&core).len(), 1);

    // An urgent report takes the batch with it.
    assert_eq!(batch::report(&core, &env, "t1", "b4: blocked, no key", 1200, &arm).unwrap(), "sent");
    let last = texts(&core).pop().unwrap();
    assert!(last.starts_with(HELD_LEAD) && last.contains(">b2: done, PR #6</hc>") && last.ends_with(">b4: blocked, no key</hc>"), "{last}");

    // The timer finds nothing left.
    assert_eq!(batch::flush(&core, &env, "t1", 1600).unwrap(), "no reports");
    assert_eq!(batch::flush(&core, &env, "t1", 1600).unwrap(), "no batch");
}

#[test]
fn the_timer_gives_the_batch_when_due() {
    let e = Env::new(Some(5));
    let env = |k: &str| e.get(k);
    let core = Fake::default();
    let arm = |_: &str, s: u64| assert_eq!(s, 240);
    batch::report(&core, &env, "t1", "b1: done", 0, &arm).unwrap(); // last = 0, so 300 is due
    batch::report(&core, &env, "t1", "b2: done", 60, &arm).unwrap();
    assert_eq!(batch::flush(&core, &env, "t1", 299).unwrap(), "not due");
    assert_eq!(batch::flush(&core, &env, "t1", 300).unwrap(), "sent");
    assert!(texts(&core)[1].ends_with(">b2: done</hc>"));
    // After the interval a plain report goes at once again.
    assert_eq!(batch::report(&core, &env, "t1", "b3: done", 700, &|_, _| panic!("no timer")).unwrap(), "sent");
}

#[test]
fn a_refused_batch_stays_held_for_the_hooks() {
    let e = Env::new(None);
    let env = |k: &str| e.get(k);
    batch::report(&Fake::default(), &env, "t1", "b1: done", 0, &|_, _| {}).unwrap();
    batch::report(&Fake::default(), &env, "t1", "b2: done", 10, &|_, _| {}).unwrap();
    let busy = Fake::default().reply("terminal.send", json!({"sent": false, "reason": "blocked"}));
    assert_eq!(batch::flush(&busy, &env, "t1", 600).unwrap(), "held (blocked)");
    let r = batch::report(&busy, &env, "t1", "b3: failed, x", 610, &|_, _| {}).unwrap();
    assert!(r.starts_with("held: t1 did not take it now (blocked)"), "{r}");
    assert_eq!(fs::read_dir(e.state.path().join("held/t1")).unwrap().count(), 2);
    let err = Fake::default().err("terminal.send", "hc: not_found");
    assert_eq!(batch::flush(&err, &env, "t1", 700).unwrap(), "no batch");
    assert!(batch::report(&err, &env, "t1", "b4: blocked", 700, &|_, _| {}).is_err());
    assert_eq!(fs::read_dir(e.state.path().join("held/t1")).unwrap().count(), 3, "an hc error loses no report");
}

#[test]
fn zero_turns_batching_off() {
    let e = Env::new(Some(0));
    let env = |k: &str| e.get(k);
    let core = Fake::default();
    for t in ["b1: done", "b2: done"] {
        assert_eq!(batch::report(&core, &env, "t1", t, 5, &|_, _| panic!("no timer")).unwrap(), "sent");
    }
    assert_eq!(texts(&core), ["b1: done", "b2: done"]);
    assert!(batch::report(&core, &env, "t1", " ", 5, &|_, _| {}).is_err());
}

#[test]
fn the_flush_command() {
    let e = Env::new(None);
    let vars = [("CONTR_STATE_DIR", path(e.state.path()))];
    let r = call(&["batch", "flush", "t1"], &vars, &Fake::default());
    assert_eq!((r.code, r.out.as_str()), (0, "no batch\n"));
}
