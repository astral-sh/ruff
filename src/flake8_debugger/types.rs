use serde::{Deserialize, Serialize};

pub type Debugger<'a> = (&'a str, &'a [&'a str]);

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DebuggerUsingType {
    Call(String),
    Import,
}
