use crate::{Db, RangedValue};
use red_knot_python_semantic::types::Type;
use red_knot_python_semantic::{HasType, SemanticModel};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::Stmt;
use ruff_text_size::{Ranged, TextRange};
use std::fmt;
use std::fmt::Formatter;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InlayHintContent<'db> {
    AssignStatement(Type<'db>),
    FunctionReturnType(Type<'db>),
}

impl<'db> InlayHintContent<'db> {
    pub const fn display(&self, db: &'db dyn Db) -> DisplayInlayHint<'_, 'db> {
        DisplayInlayHint { db, hint: self }
    }
}

pub struct DisplayInlayHint<'a, 'db> {
    db: &'db dyn Db,
    hint: &'a InlayHintContent<'db>,
}

impl fmt::Display for DisplayInlayHint<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.hint {
            InlayHintContent::AssignStatement(ty) => {
                write!(f, ": {}", ty.display(self.db.upcast()))
            }
            InlayHintContent::FunctionReturnType(ty) => {
                write!(f, " -> {}", ty.display(self.db.upcast()))
            }
        }
    }
}

pub fn get_inlay_hints(db: &dyn Db, file: File) -> Vec<RangedValue<InlayHintContent<'_>>> {
    let mut visitor = InlayHintVisitor::new(db, file);

    let ast = parsed_module(db, file);

    visitor.visit_body(ast.suite());

    let hints = visitor.hints().clone();

    hints
}

struct InlayHintVisitor<'db> {
    model: SemanticModel<'db>,
    file: File,
    hints: Vec<RangedValue<InlayHintContent<'db>>>,
}

impl<'db> InlayHintVisitor<'db> {
    fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            model: SemanticModel::new(db, file),
            file,
            hints: Vec::new(),
        }
    }

    fn hints(&self) -> &Vec<RangedValue<InlayHintContent<'db>>> {
        &self.hints
    }
}

impl SourceOrderVisitor<'_> for InlayHintVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        let file_range = |range: TextRange| FileRange::new(self.file, range);

        match stmt {
            Stmt::Assign(assign) => {
                let ty = assign.value.inferred_type(&self.model);
                for target in &assign.targets {
                    self.hints.push(RangedValue {
                        range: file_range(target.range()),
                        value: InlayHintContent::AssignStatement(ty),
                    });
                }
                return;
            }
            // TODO
            Stmt::FunctionDef(_) => {}
            Stmt::For(_) => {}
            Stmt::Expr(_) => {}
            _ => {}
        }

        source_order::walk_stmt(self, stmt);
    }
}
