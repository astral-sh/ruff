use crate::Db;
use red_knot_python_semantic::types::Type;
use red_knot_python_semantic::{HasType, SemanticModel};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};
use std::fmt;
use std::fmt::Formatter;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InlayHint<'db> {
    pub range: TextRange,
    pub content: InlayHintContent<'db>,
}

impl<'db> InlayHint<'db> {
    pub const fn display(&self, db: &'db dyn Db) -> DisplayInlayHint<'_, 'db> {
        self.content.display(db)
    }
}

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

pub fn inlay_hints(db: &dyn Db, file: File) -> Vec<InlayHint<'_>> {
    let mut visitor = InlayHintVisitor::new(db, file);

    let ast = parsed_module(db.upcast(), file);

    visitor.visit_body(ast.suite());

    visitor.hints
}

struct InlayHintVisitor<'db> {
    model: SemanticModel<'db>,
    hints: Vec<InlayHint<'db>>,
}

impl<'db> InlayHintVisitor<'db> {
    fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            model: SemanticModel::new(db.upcast(), file),
            hints: Vec::new(),
        }
    }

    fn add_hint(&mut self, range: TextRange, ty: Type<'db>) {
        self.hints.push(InlayHint {
            range,
            content: InlayHintContent::AssignStatement(ty),
        });
    }
}

impl SourceOrderVisitor<'_> for InlayHintVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    match target {
                        Expr::Tuple(tuple) => {
                            for element in &tuple.elts {
                                let element_ty = element.inferred_type(&self.model);
                                self.add_hint(element.range(), element_ty);
                            }
                        }
                        _ => {
                            let ty = assign.value.inferred_type(&self.model);
                            self.add_hint(target.range(), ty);
                        }
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    use red_knot_python_semantic::types::StringLiteralType;
    use ruff_db::files::{system_path_to_file, File};
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_text_size::TextSize;

    use crate::db::tests::TestDb;

    struct TestCase {
        db: TestDb,
        file: File,
    }

    fn test_case(content: impl AsRef<str>) -> TestCase {
        let mut db = TestDb::new();
        db.write_file("test.py", content).unwrap();

        let file = system_path_to_file(&db, "test.py").unwrap();

        TestCase { db, file }
    }

    #[test]
    fn test_assign_statement() {
        let test_case = test_case("x = 1");
        let hints = inlay_hints(&test_case.db, test_case.file);
        assert_eq!(hints.len(), 1);
        assert_eq!(
            hints[0].content,
            InlayHintContent::AssignStatement(Type::IntLiteral(1))
        );
        assert_eq!(
            hints[0].range,
            TextRange::new(TextSize::from(0), TextSize::from(1))
        );
    }

    #[test]
    fn test_tuple_assignment() {
        let test_case = test_case("x, y = (1, 'abc')");
        let hints = inlay_hints(&test_case.db, test_case.file);
        assert_eq!(hints.len(), 2);
        assert_eq!(
            hints[0].content,
            InlayHintContent::AssignStatement(Type::IntLiteral(1))
        );
        assert_eq!(
            hints[1].content,
            InlayHintContent::AssignStatement(Type::StringLiteral(StringLiteralType::new(
                &test_case.db,
                "abc"
            )))
        );
        assert_eq!(
            hints[0].range,
            TextRange::new(TextSize::from(0), TextSize::from(1))
        );
        assert_eq!(
            hints[1].range,
            TextRange::new(TextSize::from(3), TextSize::from(4))
        );
    }

    #[test]
    fn test_assign_statement_with_type_annotation() {
        let test_case = test_case("x: int = 1");
        let hints = inlay_hints(&test_case.db, test_case.file);
        assert_eq!(hints.len(), 0);
    }
}
