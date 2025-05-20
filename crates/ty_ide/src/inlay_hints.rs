use crate::Db;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::fmt;
use std::fmt::Formatter;
use ty_python_semantic::types::Type;
use ty_python_semantic::{HasType, SemanticModel};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InlayHint<'db> {
    pub position: TextSize,
    pub content: InlayHintContent<'db>,
}

impl<'db> InlayHint<'db> {
    pub const fn display(&self, db: &'db dyn Db) -> DisplayInlayHint<'_, 'db> {
        self.content.display(db)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InlayHintContent<'db> {
    Type(Type<'db>),
    ReturnType(Type<'db>),
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
            InlayHintContent::Type(ty) => {
                write!(f, ": {}", ty.display(self.db.upcast()))
            }
            InlayHintContent::ReturnType(ty) => {
                write!(f, " -> {}", ty.display(self.db.upcast()))
            }
        }
    }
}

pub fn inlay_hints(db: &dyn Db, file: File, range: TextRange) -> Vec<InlayHint<'_>> {
    let mut visitor = InlayHintVisitor::new(db, file, range);

    let ast = parsed_module(db.upcast(), file);

    visitor.visit_body(ast.suite());

    visitor.hints
}

struct InlayHintVisitor<'db> {
    model: SemanticModel<'db>,
    hints: Vec<InlayHint<'db>>,
    in_assignment: bool,
    range: TextRange,
}

impl<'db> InlayHintVisitor<'db> {
    fn new(db: &'db dyn Db, file: File, range: TextRange) -> Self {
        Self {
            model: SemanticModel::new(db.upcast(), file),
            hints: Vec::new(),
            in_assignment: false,
            range,
        }
    }

    fn add_type_hint(&mut self, position: TextSize, ty: Type<'db>) {
        self.hints.push(InlayHint {
            position,
            content: InlayHintContent::Type(ty),
        });
    }
}

impl SourceOrderVisitor<'_> for InlayHintVisitor<'_> {
    fn enter_node(&mut self, node: AnyNodeRef<'_>) -> TraversalSignal {
        if self.range.intersect(node.range()).is_some() {
            TraversalSignal::Traverse
        } else {
            TraversalSignal::Skip
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        let node = AnyNodeRef::from(stmt);

        if !self.enter_node(node).is_traverse() {
            return;
        }

        match stmt {
            Stmt::Assign(assign) => {
                self.in_assignment = true;
                for target in &assign.targets {
                    self.visit_expr(target);
                }
                self.in_assignment = false;

                return;
            }
            // TODO
            Stmt::FunctionDef(_) => {}
            Stmt::For(_) => {}
            Stmt::Expr(_) => {
                // Don't traverse into expression statements because we don't show any hints.
                return;
            }
            _ => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'_ Expr) {
        if !self.in_assignment {
            return;
        }

        match expr {
            Expr::Name(name) => {
                if name.ctx.is_store() {
                    let ty = expr.inferred_type(&self.model);
                    self.add_type_hint(expr.range().end(), ty);
                }
            }
            _ => {
                source_order::walk_expr(self, expr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use insta::assert_snapshot;
    use ruff_db::{
        files::{File, system_path_to_file},
        source::source_text,
    };
    use ruff_text_size::TextSize;

    use crate::db::tests::TestDb;

    use ruff_db::system::{DbWithWritableSystem, SystemPathBuf};
    use ty_python_semantic::{
        Program, ProgramSettings, PythonPath, PythonPlatform, PythonVersionWithSource,
        SearchPathSettings,
    };

    pub(super) fn inlay_hint_test(source: &str) -> InlayHintTest {
        const START: &str = "<START>";
        const END: &str = "<END>";

        let mut db = TestDb::new();

        let start = source.find(START);
        let end = source
            .find(END)
            .map(|x| if start.is_some() { x - START.len() } else { x })
            .unwrap_or(source.len());

        let range = TextRange::new(
            TextSize::try_from(start.unwrap_or_default()).unwrap(),
            TextSize::try_from(end).unwrap(),
        );

        let source = source.replace(START, "");
        let source = source.replace(END, "");

        db.write_file("main.py", source)
            .expect("write to memory file system to be successful");

        let file = system_path_to_file(&db, "main.py").expect("newly written file to existing");

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersionWithSource::default(),
                python_platform: PythonPlatform::default(),
                search_paths: SearchPathSettings {
                    extra_paths: vec![],
                    src_roots: vec![SystemPathBuf::from("/")],
                    custom_typeshed: None,
                    python_path: PythonPath::KnownSitePackages(vec![]),
                },
            },
        )
        .expect("Default settings to be valid");

        InlayHintTest { db, file, range }
    }

    pub(super) struct InlayHintTest {
        pub(super) db: TestDb,
        pub(super) file: File,
        pub(super) range: TextRange,
    }

    impl InlayHintTest {
        fn inlay_hints(&self) -> String {
            let hints = inlay_hints(&self.db, self.file, self.range);

            let mut buf = source_text(&self.db, self.file).as_str().to_string();

            let mut offset = 0;

            for hint in hints {
                let end_position = (hint.position.to_u32() as usize) + offset;
                let hint_str = format!("[{}]", hint.display(&self.db));
                buf.insert_str(end_position, &hint_str);
                offset += hint_str.len();
            }

            buf
        }
    }

    #[test]
    fn test_assign_statement() {
        let test = inlay_hint_test("x = 1");

        assert_snapshot!(test.inlay_hints(), @r"
        x[: Literal[1]] = 1
        ");
    }

    #[test]
    fn test_tuple_assignment() {
        let test = inlay_hint_test("x, y = (1, 'abc')");

        assert_snapshot!(test.inlay_hints(), @r#"
        x[: Literal[1]], y[: Literal["abc"]] = (1, 'abc')
        "#);
    }

    #[test]
    fn test_nested_tuple_assignment() {
        let test = inlay_hint_test("x, (y, z) = (1, ('abc', 2))");

        assert_snapshot!(test.inlay_hints(), @r#"
        x[: Literal[1]], (y[: Literal["abc"]], z[: Literal[2]]) = (1, ('abc', 2))
        "#);
    }

    #[test]
    fn test_assign_statement_with_type_annotation() {
        let test = inlay_hint_test("x: int = 1");

        assert_snapshot!(test.inlay_hints(), @r"
        x: int = 1
        ");
    }

    #[test]
    fn test_assign_statement_out_of_range() {
        let test = inlay_hint_test("<START>x = 1<END>\ny = 2");

        assert_snapshot!(test.inlay_hints(), @r"
        x[: Literal[1]] = 1
        y = 2
        ");
    }
}
