//! Due dates of owner to-dos (#214). On its day, or later when the coordinator was not running, an open to-do
//! with a due date comes back to the coordinator that pinned it as `contr: due today: <ID> <text>`, one time.
use crate::owner::Store;

/// An empty due date, or a valid YYYY-MM-DD one.
pub fn check(due: &str) -> Result<(), String> {
    if due.is_empty()
        || (due.len() == 10 && chrono::NaiveDate::parse_from_str(due, "%Y-%m-%d").is_ok())
    {
        return Ok(());
    }
    Err(format!("owner: due {due:?} is not a date YYYY-MM-DD"))
}

/// Today as YYYY-MM-DD. `CONTR_TODAY` sets it for tests; else it is the local date.
pub fn today(env: &dyn Fn(&str) -> String) -> String {
    let t = env("CONTR_TODAY");
    if !t.is_empty() {
        return t;
    }
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// The notes for the open to-dos of `terminal` that are due on or before `today` and were not told. It marks
/// them told. No to-do is due gives an empty text, and then nothing is written.
pub fn take(store: &Store, terminal: &str, today: &str) -> Result<String, String> {
    let is_due = |t: &crate::owner::Todo| {
        t.done.is_none()
            && !t.due_told
            && !t.due.is_empty()
            && t.coordinator == terminal
            && t.due.as_str() <= today
    };
    if terminal.is_empty() || !store.read()?.todos.iter().any(is_due) {
        return Ok(String::new());
    }
    let mut lines = Vec::new();
    store.update(|st| {
        for t in st.todos.iter_mut().filter(|t| is_due(t)) {
            t.due_told = true;
            lines.push(format!("contr: due today: {} {}", t.id, t.text));
        }
        Ok(())
    })?;
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owner::Todo;

    fn store(dir: &tempfile::TempDir) -> Store {
        let mut s = Store::new(dir.path().join("owner.json"));
        s.now = Some(|| "2026-11-16T10:00:00Z".into());
        s
    }

    fn pin(s: &Store, coordinator: &str, text: &str, due: &str) -> String {
        s.pin(Todo {
            project: "p".into(),
            coordinator: coordinator.into(),
            text: text.into(),
            due: due.into(),
            ..Todo::default()
        })
        .unwrap()
        .id
    }

    #[test]
    fn a_due_date_must_be_a_day() {
        assert!(check("").is_ok());
        assert!(check("2026-11-20").is_ok());
        for bad in [
            "2026-13-01",
            "20-11-2026",
            "2026-11-20T10:00",
            "tomorrow",
            "2026-1-5",
        ] {
            assert!(check(bad).is_err(), "{bad}");
        }
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        assert!(s
            .pin(Todo {
                text: "x".into(),
                due: "soon".into(),
                ..Todo::default()
            })
            .is_err());
        assert!(s.read().unwrap().todos.is_empty());
    }

    #[test]
    fn a_due_to_do_comes_back_one_time_on_its_day_to_its_coordinator() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let t1 = pin(&s, "c1", "check the deploy", "2026-11-18");
        pin(&s, "c1", "no date", "");
        pin(&s, "c2", "other coordinator", "2026-11-18");
        let t4 = pin(&s, "c1", "done before", "2026-11-18");
        s.unpin(&[t4]).unwrap();
        let t5 = pin(&s, "c1", "missed yesterday", "2026-11-17");

        assert_eq!(take(&s, "c1", "2026-11-16").unwrap(), "");
        assert_eq!(
            take(&s, "c1", "2026-11-18").unwrap(),
            format!(
                "contr: due today: {t1} check the deploy\ncontr: due today: {t5} missed yesterday"
            )
        );
        assert_eq!(take(&s, "c1", "2026-11-18").unwrap(), "");
        assert_eq!(take(&s, "", "2026-11-18").unwrap(), "");
        let open = s.read().unwrap().open("p").1;
        assert!(
            open.iter().any(|t| t.id == t1 && t.due == "2026-11-18"),
            "the to-do stays open with its date"
        );
    }

    #[test]
    fn today_comes_from_the_environment_for_tests() {
        assert_eq!(
            today(&|k: &str| if k == "CONTR_TODAY" {
                "2026-01-02".into()
            } else {
                String::new()
            }),
            "2026-01-02"
        );
        assert_eq!(today(&|_: &str| String::new()).len(), 10);
    }
}
