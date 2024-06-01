use crate::db::Upcast;
use crate::salsa_db::source::File;
use crate::salsa_db::{semantic, source};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::Expr;
use std::sync::Arc;
use tracing::warn;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxCheck {
    inner: Arc<SyntaxCheckInner>,
}

impl SyntaxCheck {
    pub fn new(diagnostics: Vec<String>) -> Self {
        Self {
            inner: Arc::new(SyntaxCheckInner { diagnostics }),
        }
    }

    #[allow(unused)]
    pub fn diagnostics(&self) -> &[String] {
        &self.inner.diagnostics
    }
}

#[derive(Debug, Eq, PartialEq)]
struct SyntaxCheckInner {
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PhysicalLinesCheck {
    inner: Arc<PhysicalLinesCheckInner>,
}

#[derive(Eq, PartialEq, Debug)]
struct PhysicalLinesCheckInner {
    diagnostics: Vec<String>,
}

impl PhysicalLinesCheck {
    pub fn new(diagnostics: Vec<String>) -> Self {
        Self {
            inner: Arc::new(PhysicalLinesCheckInner { diagnostics }),
        }
    }

    #[allow(unused)]
    pub fn diagnostics(&self) -> &[String] {
        &self.inner.diagnostics
    }
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

    SyntaxCheck::new(visitor.diagnostics)
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

    PhysicalLinesCheck::new(diagnostics)
}

#[salsa::jar(db=Db)]
pub struct Jar(check_physical_lines, check_syntax);

pub trait Db: semantic::Db + salsa::DbWithJar<Jar> + Upcast<dyn semantic::Db> {}
