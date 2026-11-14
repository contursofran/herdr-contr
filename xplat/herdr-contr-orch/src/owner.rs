//! What waits for the owner: questions that only the owner can answer, and to-dos that the owner must do or
//! should know. The items are in one JSON file that every coordinator writes and the chat reads. A lock file and
//! an atomic rename keep concurrent writers safe. Ported from contr's `internal/owner`; the file is the same.
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::jsonfile;

/// A time as the file holds it (RFC 3339), or None. Go's zero time counts as None.
fn time<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.filter(|t| !t.is_empty() && !t.starts_with("0001-01-01")))
}

/// A list that the file may hold as null.
fn list<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

/// A decision that waits for the owner.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Question {
    /// D1, D2, …
    pub id: String,
    pub project: String,
    /// The hc terminal that asked; the answer goes there.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub coordinator: String,
    /// The worker, issue or PR it is about.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub from: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    pub question: String,
    #[serde(deserialize_with = "list", skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// Index in `options`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<i64>,
    /// What each option means; `impacts[i]` is for `options[i]`.
    #[serde(deserialize_with = "list", skip_serializing_if = "Vec::is_empty")]
    pub impacts: Vec<String>,
    /// Issue or PR numbers, file paths, URLs that help to decide.
    #[serde(deserialize_with = "list", skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
    /// A short excerpt of the text that raised the question.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(deserialize_with = "time")]
    pub asked: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub answer: String,
    /// owner or coordinator.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub answered_by: String,
    #[serde(deserialize_with = "time", skip_serializing_if = "Option::is_none")]
    pub answered: Option<String>,
}

/// Something the owner must do ("do") or should know ("know").
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Todo {
    /// T1, T2, …
    pub id: String,
    pub project: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub coordinator: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub from: String,
    pub kind: String,
    pub text: String,
    #[serde(deserialize_with = "time")]
    pub pinned: Option<String>,
    #[serde(deserialize_with = "time", skip_serializing_if = "Option::is_none")]
    pub done: Option<String>,
}

/// The whole file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    #[serde(deserialize_with = "list")]
    pub questions: Vec<Question>,
    #[serde(deserialize_with = "list")]
    pub todos: Vec<Todo>,
    pub next_question: i64,
    pub next_todo: i64,
}

impl Default for State {
    fn default() -> State {
        State { questions: Vec::new(), todos: Vec::new(), next_question: 1, next_todo: 1 }
    }
}

impl State {
    /// The open questions and to-dos of `project`; an empty project means all projects.
    pub fn open(&self, project: &str) -> (Vec<Question>, Vec<Todo>) {
        let of = |p: &str| project.is_empty() || p == project;
        (
            self.questions.iter().filter(|q| q.answer.is_empty() && of(&q.project)).cloned().collect(),
            self.todos.iter().filter(|t| t.done.is_none() && of(&t.project)).cloned().collect(),
        )
    }
}

/// The time now as the file holds it.
fn now_text() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

/// The file at `path`. `now` is the clock; None means the time now.
pub struct Store {
    pub path: PathBuf,
    pub now: Option<fn() -> String>,
}

impl Store {
    pub fn new(path: impl Into<PathBuf>) -> Store {
        Store { path: path.into(), now: None }
    }

    fn now(&self) -> String {
        self.now.map(|f| f()).unwrap_or_else(now_text)
    }

    /// The state. A missing file is an empty state.
    pub fn read(&self) -> Result<State, String> {
        let data = match fs::read(&self.path) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(State::default()),
            Err(e) => return Err(format!("owner: {}: {e}", self.path.display())),
        };
        let mut st: State = serde_json::from_slice(&data).map_err(|e| format!("owner: {}: {e}", self.path.display()))?;
        st.next_question = st.next_question.max(1);
        st.next_todo = st.next_todo.max(1);
        Ok(st)
    }

    /// Runs `f` on the state under the lock and writes the result. An error from `f` writes nothing.
    fn update(&self, f: impl FnOnce(&mut State) -> Result<(), String>) -> Result<(), String> {
        let io_err = |e: io::Error| format!("owner: {}: {e}", self.path.display());
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            make_private_dir(dir).map_err(io_err)?;
        }
        let mut lock_path = self.path.clone().into_os_string();
        lock_path.push(".lock");
        let _lock = Lock::take(Path::new(&lock_path)).map_err(|e| format!("owner: lock: {e}"))?;
        let mut st = self.read()?;
        f(&mut st)?;
        let mut data = serde_json::to_vec_pretty(&st).map_err(|e| e.to_string())?;
        data.push(b'\n');
        jsonfile::write(&self.path, &data, 0o600).map_err(io_err)
    }

    /// Adds a question and returns it with its id.
    pub fn ask(&self, mut q: Question) -> Result<Question, String> {
        if q.question.is_empty() {
            return Err("owner: a question needs text".into());
        }
        if let Some(r) = q.recommended {
            if r < 0 || r >= q.options.len() as i64 {
                return Err(format!("owner: recommended {r} is not an option index"));
            }
        }
        self.update(|st| {
            q.id = format!("D{}", st.next_question);
            st.next_question += 1;
            q.asked = Some(self.now());
            q.answer.clear();
            q.answered_by.clear();
            q.answered = None;
            st.questions.push(q.clone());
            Ok(())
        })?;
        Ok(q)
    }

    /// Adds a to-do and returns it with its id. The kind defaults to "do".
    pub fn pin(&self, mut t: Todo) -> Result<Todo, String> {
        if t.text.is_empty() {
            return Err("owner: a to-do needs text".into());
        }
        if t.kind.is_empty() {
            t.kind = "do".into();
        }
        if t.kind != "do" && t.kind != "know" {
            return Err(format!("owner: kind {:?} is not do or know", t.kind));
        }
        self.update(|st| {
            t.id = format!("T{}", st.next_todo);
            st.next_todo += 1;
            t.pinned = Some(self.now());
            t.done = None;
            st.todos.push(t.clone());
            Ok(())
        })?;
        Ok(t)
    }

    /// Records the answer of open questions. `by` is "owner" or "coordinator". An id that is not an open
    /// question is an error, and then nothing changes.
    pub fn decide(&self, ids: &[String], answer: &str, by: &str) -> Result<(), String> {
        if ids.is_empty() {
            return Err("owner: no question ids".into());
        }
        if answer.is_empty() {
            return Err("owner: an answer needs text".into());
        }
        if by != "owner" && by != "coordinator" {
            return Err(format!("owner: by {by:?} is not owner or coordinator"));
        }
        self.update(|st| {
            for id in ids {
                if !st.questions.iter().any(|q| &q.id == id && q.answer.is_empty()) {
                    return Err(format!("owner: no open question {id}"));
                }
            }
            let at = self.now();
            for q in st.questions.iter_mut().filter(|q| ids.contains(&q.id)) {
                q.answer = answer.to_string();
                q.answered_by = by.to_string();
                q.answered = Some(at.clone());
            }
            Ok(())
        })
    }

    /// Marks open to-dos done. An id that is not an open to-do is an error, and then nothing changes.
    pub fn unpin(&self, ids: &[String]) -> Result<(), String> {
        if ids.is_empty() {
            return Err("owner: no to-do ids".into());
        }
        self.update(|st| {
            for id in ids {
                if !st.todos.iter().any(|t| &t.id == id && t.done.is_none()) {
                    return Err(format!("owner: no open to-do {id}"));
                }
            }
            let at = self.now();
            for t in st.todos.iter_mut().filter(|t| ids.contains(&t.id)) {
                t.done = Some(at.clone());
            }
            Ok(())
        })
    }
}

fn make_private_dir(dir: &Path) -> io::Result<()> {
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir)
}

/// An exclusive lock on a file; it ends when the value drops (the file closes).
pub(crate) struct Lock(#[allow(dead_code)] fs::File);

impl Lock {
    pub(crate) fn take(path: &Path) -> io::Result<Lock> {
        let mut opts = OpenOptions::new();
        opts.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let f = opts.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            // SAFETY: the descriptor is open for the life of `f`.
            if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Lock(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = Store { path: dir.path().join("contr/owner.json"), now: Some(|| "2026-11-13T10:00:00-03:00".to_string()) };
        (dir, s)
    }
    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_missing_file_is_an_empty_state() {
        let (_d, s) = store();
        assert_eq!(s.read().unwrap(), State::default());
        assert!(!s.path.exists());
    }

    #[test]
    fn ask_gives_ids_in_order_and_clears_an_answer_that_came_with_it() {
        let (_d, s) = store();
        let q = s.ask(Question { project: "app".into(), question: "Which one?".into(), options: ids(&["a", "b"]), recommended: Some(1),
            answer: "x".into(), answered_by: "owner".into(), ..Question::default() }).unwrap();
        assert_eq!((q.id.as_str(), q.answer.as_str(), q.asked.as_deref()), ("D1", "", Some("2026-11-13T10:00:00-03:00")));
        assert_eq!(s.ask(Question { question: "And this?".into(), ..Question::default() }).unwrap().id, "D2");
        let st = s.read().unwrap();
        assert_eq!((st.questions.len(), st.next_question), (2, 3));
        assert_eq!(st.questions[0], q);
    }

    #[test]
    fn ask_refuses_no_text_and_a_recommended_that_is_no_option() {
        let (_d, s) = store();
        assert_eq!(s.ask(Question::default()).unwrap_err(), "owner: a question needs text");
        for r in [-1, 2] {
            let err = s.ask(Question { question: "q".into(), options: ids(&["a", "b"]), recommended: Some(r), ..Question::default() }).unwrap_err();
            assert_eq!(err, format!("owner: recommended {r} is not an option index"));
        }
        assert!(!s.path.exists());
    }

    #[test]
    fn pin_defaults_to_do_and_refuses_another_kind() {
        let (_d, s) = store();
        let t = s.pin(Todo { project: "app".into(), text: "Test the build".into(), ..Todo::default() }).unwrap();
        assert_eq!((t.id.as_str(), t.kind.as_str()), ("T1", "do"));
        assert_eq!(s.pin(Todo { text: "CI is slow".into(), kind: "know".into(), ..Todo::default() }).unwrap().id, "T2");
        assert_eq!(s.pin(Todo::default()).unwrap_err(), "owner: a to-do needs text");
        assert_eq!(s.pin(Todo { text: "x".into(), kind: "maybe".into(), ..Todo::default() }).unwrap_err(), "owner: kind \"maybe\" is not do or know");
    }

    #[test]
    fn decide_answers_open_questions_or_changes_nothing() {
        let (_d, s) = store();
        s.ask(Question { question: "one".into(), ..Question::default() }).unwrap();
        s.ask(Question { question: "two".into(), ..Question::default() }).unwrap();
        assert_eq!(s.decide(&ids(&["D1", "D9"]), "yes", "owner").unwrap_err(), "owner: no open question D9");
        assert!(s.read().unwrap().questions.iter().all(|q| q.answer.is_empty()));
        s.decide(&ids(&["D1"]), "yes", "coordinator").unwrap();
        let st = s.read().unwrap();
        assert_eq!((st.questions[0].answer.as_str(), st.questions[0].answered_by.as_str()), ("yes", "coordinator"));
        assert!(st.questions[0].answered.is_some() && st.questions[1].answered.is_none());
        assert_eq!(s.decide(&ids(&["D1"]), "again", "owner").unwrap_err(), "owner: no open question D1");
        assert_eq!(s.decide(&[], "x", "owner").unwrap_err(), "owner: no question ids");
        assert_eq!(s.decide(&ids(&["D2"]), "", "owner").unwrap_err(), "owner: an answer needs text");
        assert_eq!(s.decide(&ids(&["D2"]), "x", "someone").unwrap_err(), "owner: by \"someone\" is not owner or coordinator");
    }

    #[test]
    fn unpin_marks_open_todos_done_or_changes_nothing() {
        let (_d, s) = store();
        s.pin(Todo { text: "one".into(), ..Todo::default() }).unwrap();
        s.pin(Todo { text: "two".into(), ..Todo::default() }).unwrap();
        assert_eq!(s.unpin(&ids(&["T2", "T7"])).unwrap_err(), "owner: no open to-do T7");
        assert!(s.read().unwrap().todos.iter().all(|t| t.done.is_none()));
        s.unpin(&ids(&["T2"])).unwrap();
        assert_eq!(s.unpin(&ids(&["T2"])).unwrap_err(), "owner: no open to-do T2");
        assert_eq!(s.unpin(&[]).unwrap_err(), "owner: no to-do ids");
        let (qs, ts) = s.read().unwrap().open("");
        assert_eq!((qs.len(), ts.len(), ts[0].id.as_str()), (0, 1, "T1"));
    }

    #[test]
    fn open_gives_the_open_items_of_one_project() {
        let (_d, s) = store();
        s.ask(Question { project: "a".into(), question: "qa".into(), ..Question::default() }).unwrap();
        s.ask(Question { project: "b".into(), question: "qb".into(), ..Question::default() }).unwrap();
        s.pin(Todo { project: "b".into(), text: "tb".into(), ..Todo::default() }).unwrap();
        s.decide(&ids(&["D1"]), "ok", "owner").unwrap();
        let st = s.read().unwrap();
        assert_eq!(st.open("a"), (vec![], vec![]));
        let (qs, ts) = st.open("b");
        assert_eq!((qs[0].id.as_str(), ts[0].id.as_str()), ("D2", "T1"));
        assert_eq!(st.open("").0.len(), 1);
    }

    #[test]
    fn reads_the_file_that_the_go_tool_wrote() {
        let (_d, s) = store();
        fs::create_dir_all(s.path.parent().unwrap()).unwrap();
        fs::write(&s.path, r#"{"questions":[{"id":"D4","project":"app","coordinator":"t2nw","question":"Go?","options":["yes","no"],
            "recommended":0,"asked":"2026-11-13T09:00:00.123456-03:00","answer":"yes","answered_by":"owner","answered":"2026-11-13T09:05:00-03:00"},
            {"id":"D5","project":"app","question":"Open?","asked":"2026-11-13T09:10:00-03:00","answered":"0001-01-01T00:00:00Z"}],
            "todos":null,"next_question":0,"next_todo":3}"#).unwrap();
        let st = s.read().unwrap();
        assert_eq!((st.next_question, st.next_todo, st.todos.len()), (1, 3, 0));
        assert_eq!(st.questions[0].recommended, Some(0));
        assert_eq!(st.questions[1].answered, None);
        assert_eq!(st.open("app").0[0].id, "D5");
        fs::write(&s.path, "{").unwrap();
        assert!(s.read().unwrap_err().contains("owner.json"));
        assert!(s.ask(Question { question: "q".into(), ..Question::default() }).is_err());
        assert_eq!(fs::read_to_string(&s.path).unwrap(), "{");
    }

    #[test]
    fn the_file_omits_what_is_empty() {
        let (_d, s) = store();
        s.ask(Question { project: "app".into(), question: "q".into(), ..Question::default() }).unwrap();
        s.pin(Todo { project: "app".into(), text: "t".into(), ..Todo::default() }).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&fs::read(&s.path).unwrap()).unwrap();
        let keys = |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(keys(&v["questions"][0]), ["asked", "id", "project", "question"]);
        assert_eq!(keys(&v["todos"][0]), ["id", "kind", "pinned", "project", "text"]);
        assert_eq!((v["next_question"].as_i64(), v["next_todo"].as_i64()), (Some(2), Some(2)));
    }

    #[test]
    fn writers_at_once_lose_no_item() {
        let (_d, s) = store();
        let path = s.path.clone();
        let threads: Vec<_> = (0..8).map(|i| {
            let path = path.clone();
            std::thread::spawn(move || {
                let s = Store::new(path);
                for j in 0..5 {
                    s.ask(Question { question: format!("q{i}-{j}"), ..Question::default() }).unwrap();
                }
            })
        }).collect();
        for t in threads {
            t.join().unwrap();
        }
        let st = s.read().unwrap();
        let mut got: Vec<&str> = st.questions.iter().map(|q| q.id.as_str()).collect();
        got.sort();
        got.dedup();
        assert_eq!((st.questions.len(), got.len(), st.next_question), (40, 40, 41));
    }
}
