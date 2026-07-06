//! Method harvest: `def foo(self, …):` → a name-only
//! [`ruff_spo_triplet::Function`].
//!
//! v0 is schema-only (SPEC-5 Part A): body facts (reads/writes/calls/raises/
//! traverses) are left empty. The Action inventory starts as names-only —
//! a body-pass frontend (mirroring `ruff_python_spo::functions` /
//! `ruff_ruby_spo::functions`) is a follow-up, not this crate's job yet.

use ruff_python_ast::StmtFunctionDef;
use ruff_spo_triplet::Function;

/// Harvest a method definition into a name-only [`Function`].
#[must_use]
pub(crate) fn analyze_method(func: &StmtFunctionDef) -> Function {
    Function {
        name: func.name.id.as_str().to_string(),
        ..Default::default()
    }
}
