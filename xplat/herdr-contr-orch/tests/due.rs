//! Due to-dos (#214): the coordinator's Stop hook gives a to-do on its day, one time. The clock is CONTR_TODAY
//! and the owner file is temporary.
mod common;

use common::{call_with, path, Fake};
use orch::owner::{Store, Todo};
use serde_json::json;

#[test]
fn the_stop_hook_gives_a_due_to_do_one_time_on_its_day() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("owner.json");
    let store = Store::new(&file);
    let t = store.pin(Todo { project: "p".into(), coordinator: "t1".into(), text: "remove the flag".into(), due: "2026-11-20".into(), ..Todo::default() }).unwrap();
    let stop = |today: &str| {
        let vars = [("HC_TERMINAL_ID", "t1"), ("CONTR_STATE_DIR", path(dir.path())), ("CONTR_OWNER_FILE", path(&file)), ("CONTR_TODAY", today)];
        call_with(&["hook", "stop"], &vars, &Fake::default(), "{}")
    };
    let before = stop("2026-11-19");
    assert_eq!((before.code, before.out.as_str()), (0, ""));
    let on = stop("2026-11-20");
    assert_eq!(on.code, 0);
    let reason = format!("contr: due today: {} remove the flag", t.id);
    assert_eq!(on.out.trim(), json!({"decision": "block", "reason": reason}).to_string());
    assert_eq!(stop("2026-11-20").out, "");
    // owner open --json gives the date.
    let open = call_with(&["owner", "open", "--json"], &[("CONTR_OWNER_FILE", path(&file))], &Fake::default(), "");
    assert!(open.out.contains("\"due\":\"2026-11-20\""), "{}", open.out);
}
