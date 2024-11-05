use ruff_text_size::TextRange;
use salsa::Accumulator as _;

use crate::{
    files::File,
    source::{line_index, source_text},
    Db,
};

pub trait Diagnostic: Send + Sync + std::fmt::Debug {
    fn rule(&self) -> &str;

    fn message(&self) -> std::borrow::Cow<str>;

    fn file(&self) -> File;

    fn range(&self) -> Option<TextRange>;

    fn severity(&self) -> Severity;
}

#[derive(Debug, Clone, Copy)]
pub enum Severity {
    Info,
    Error,
}

#[salsa::accumulator]
pub struct CompileDiagnostic(std::sync::Arc<dyn Diagnostic>);

impl CompileDiagnostic {
    pub fn report<T>(db: &dyn Db, diagnostic: T)
    where
        T: Diagnostic + 'static,
    {
        Self(std::sync::Arc::new(diagnostic)).accumulate(db);
    }

    pub fn display<'a>(&'a self, db: &'a dyn Db) -> DisplayDiagnostic<'a> {
        DisplayDiagnostic {
            db,
            diagnostic: &*self.0,
        }
    }
}

impl Diagnostic for CompileDiagnostic {
    fn rule(&self) -> &str {
        self.0.rule()
    }

    fn message(&self) -> std::borrow::Cow<str> {
        self.0.message()
    }

    fn file(&self) -> File {
        self.0.file()
    }

    fn range(&self) -> Option<TextRange> {
        self.0.range()
    }

    fn severity(&self) -> Severity {
        self.0.severity()
    }
}

pub struct DisplayDiagnostic<'db> {
    db: &'db dyn Db,
    diagnostic: &'db dyn Diagnostic,
}

impl<'db> DisplayDiagnostic<'db> {
    pub fn new(db: &'db dyn Db, diagnostic: &'db dyn Diagnostic) -> Self {
        Self { db, diagnostic }
    }
}

impl std::fmt::Display for DisplayDiagnostic<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.diagnostic.severity() {
            Severity::Info => f.write_str("info")?,
            Severity::Error => f.write_str("error")?,
        }

        write!(
            f,
            "[{rule}] {path}",
            rule = self.diagnostic.rule(),
            path = self.diagnostic.file().path(self.db)
        )?;

        if let Some(range) = self.diagnostic.range() {
            let index = line_index(self.db, self.diagnostic.file());
            let source = source_text(self.db, self.diagnostic.file());

            let start = index.source_location(range.start(), &source);

            write!(f, ":{line}:{col}", line = start.row, col = start.column)?;
        }

        write!(f, " {message}", message = self.diagnostic.message())
    }
}

impl<T> Diagnostic for Box<T>
where
    T: Diagnostic,
{
    fn rule(&self) -> &str {
        (**self).rule()
    }

    fn message(&self) -> std::borrow::Cow<str> {
        (**self).message()
    }

    fn file(&self) -> File {
        (**self).file()
    }

    fn range(&self) -> Option<TextRange> {
        (**self).range()
    }

    fn severity(&self) -> Severity {
        (**self).severity()
    }
}

impl<T> Diagnostic for std::sync::Arc<T>
where
    T: Diagnostic,
{
    fn rule(&self) -> &str {
        (**self).rule()
    }

    fn message(&self) -> std::borrow::Cow<str> {
        (**self).message()
    }

    fn file(&self) -> File {
        (**self).file()
    }

    fn range(&self) -> Option<TextRange> {
        (**self).range()
    }

    fn severity(&self) -> Severity {
        (**self).severity()
    }
}
