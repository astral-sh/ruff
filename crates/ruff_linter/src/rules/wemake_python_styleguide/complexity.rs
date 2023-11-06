use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Stmt, StmtClassDef};
use ruff_python_ast::identifier::Identifier;

#[violation]
pub struct TooManyBaseClasses {
    bases: usize,
    max_bases: usize,
}

impl Violation for TooManyBaseClasses {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyBaseClasses {
            bases,
            max_bases
        } = self;
        format!("Too many base classes: ({bases} > {max_bases})")
    }
}

pub(crate) fn too_many_base_classes(class_def: &StmtClassDef) -> Option<Diagnostic> {
    let bases = class_def.bases().len();
    if bases > 3 {
        Some(Diagnostic::new(TooManyBaseClasses { bases, max_bases: 3 }, class_def.identifier()))
    } else { None }
}