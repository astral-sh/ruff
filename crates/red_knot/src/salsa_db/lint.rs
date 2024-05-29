use crate::db::Upcast;
use crate::salsa_db::source::File;
use crate::salsa_db::{semantic, source};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::Expr;
use tracing::warn;

#[salsa::tracked(jar=Jar)]
pub struct SyntaxCheck {
    #[returned_ref]
    pub diagnostics: Vec<String>,
}

#[salsa::tracked(jar=Jar)]
pub struct PhysicalLinesCheck {
    #[returned_ref]
    pub diagnostics: Vec<String>,
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar)]
pub fn check_syntax(db: &dyn Db, file: File) -> SyntaxCheck {
    // TODO I haven't looked into how many rules are pure syntax checks.
    //   It may be necessary to at least give access to a simplified semantic model.
    struct SyntaxChecker {
        diagnostics: Vec<String>,
    }

    impl Visitor<'_> for SyntaxChecker {
        fn visit_expr(&mut self, expr: &'_ Expr) {
            if let Expr::Name(name) = expr {
                if &name.id == "a" {
                    self.diagnostics.push("Use of name a".to_string());
                }
            }
        }
    }

    let parsed = source::parse(db.upcast(), file);

    let mut visitor = SyntaxChecker {
        diagnostics: Vec::new(),
    };

    visitor.visit_body(&parsed.ast().body);

    SyntaxCheck::new(db, visitor.diagnostics)
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar)]
pub fn check_physical_lines(db: &dyn Db, file: File) -> PhysicalLinesCheck {
    let source = file.source(db.upcast());
    let text = source.text();

    let mut diagnostics = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        if line.chars().count() > 88 {
            diagnostics.push(format!("Line {} too long", line_number + 1));
        }
    }

    PhysicalLinesCheck::new(db, diagnostics)
}

#[salsa::jar(db=Db)]
pub struct Jar(
    PhysicalLinesCheck,
    SyntaxCheck,
    check_physical_lines,
    check_syntax,
);

pub trait Db: semantic::Db + salsa::DbWithJar<Jar> + Upcast<dyn semantic::Db> {}
