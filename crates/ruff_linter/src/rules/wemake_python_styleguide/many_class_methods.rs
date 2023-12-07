use ruff_python_ast::{self as ast};
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextRange;
use ruff_python_semantic::analyze::visibility;

use crate::checkers::ast::Checker;


#[violation]
pub struct TooManyMethods {
    methods: usize,
    max_methods: usize,
}


impl Violation for TooManyMethods {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyMethods {
            methods,
            max_methods
        } = self;
        format!("Found too many methods: ({methods} > {max_methods})")
    }
}


pub(crate) fn too_many_methods(checker: &mut Checker, class_def: &ast::StmtClassDef) -> Option<Diagnostic> {
    let mut methods = 0;

    for stmt in class_def.body.iter() {
        if let ast::Stmt::FunctionDef(ast::StmtFunctionDef {decorator_list, ..}) = stmt {
            // Ignore any functions that are `@overload`.
            if visibility::is_overload(decorator_list, checker.semantic()) {
                continue;
            } else {
                methods += 1
            }
        }
    }

    if methods > 2 {
        Some(Diagnostic::new(TooManyMethods { methods, max_methods: 2 }, TextRange::default()))
    } else { None }
}
