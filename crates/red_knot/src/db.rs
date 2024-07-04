use red_knot_python_semantic::Db as SemanticDb;
use ruff_db::Upcast;
use salsa::DbWithJar;

use crate::lint::{lint_semantic, lint_syntax, unwind_if_cancelled};

pub trait Db: DbWithJar<Jar> + SemanticDb + Upcast<dyn SemanticDb> {}

#[salsa::jar(db=Db)]
pub struct Jar(lint_syntax, lint_semantic, unwind_if_cancelled);
