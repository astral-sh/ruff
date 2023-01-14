use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DebuggerUsingType {
    Call(String),
    Import(String),
}
