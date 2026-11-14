//! What the tests of the command line share: a fake hc core and a run of the command line on it.
#![allow(dead_code)]
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use orch::cli::run;
use orch::workers::Core;
use serde_json::{json, Value};

/// A fake hc core: it records each call, and answers a method with its reply or its error.
#[derive(Default)]
pub struct Fake {
    calls: RefCell<Vec<(String, Value)>>,
    replies: HashMap<String, Value>,
    errs: HashMap<String, String>,
}

impl Fake {
    pub fn reply(mut self, method: &str, result: Value) -> Fake {
        self.replies.insert(method.into(), result);
        self
    }
    pub fn err(mut self, method: &str, e: &str) -> Fake {
        self.errs.insert(method.into(), e.into());
        self
    }
    pub fn calls(&self) -> Vec<(String, Value)> {
        self.calls.borrow().clone()
    }
    pub fn methods(&self) -> Vec<String> {
        self.calls.borrow().iter().map(|(m, _)| m.clone()).collect()
    }
}

impl Core for Fake {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.calls.borrow_mut().push((method.to_string(), params));
        if let Some(e) = self.errs.get(method) {
            return Err(e.clone());
        }
        Ok(self.replies.get(method).cloned().unwrap_or_else(|| json!({})))
    }
}

pub struct Run {
    pub code: i32,
    pub out: String,
    pub err: String,
}

pub fn call(args: &[&str], vars: &[(&str, &str)], core: &dyn Core) -> Run {
    call_with(args, vars, core, "")
}

/// A run with this text on stdin.
pub fn call_with(args: &[&str], vars: &[(&str, &str)], core: &dyn Core, input: &str) -> Run {
    let vars: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let env = move |k: &str| vars.get(k).cloned().unwrap_or_default();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, &env, &mut input.as_bytes(), &mut out, &mut err, core);
    Run { code, out: String::from_utf8(out).unwrap(), err: String::from_utf8(err).unwrap() }
}

pub fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}
