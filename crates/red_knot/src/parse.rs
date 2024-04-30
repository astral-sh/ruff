use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use ruff_python_ast as ast;
use ruff_python_parser::{Mode, ParseError};
use ruff_text_size::{Ranged, TextRange};

use crate::cache::KeyValueCache;
use crate::db::{HasJar, QueryResult, SourceDb, SourceJar};
use crate::files::FileId;

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    inner: Arc<ParsedInner>,
}

#[derive(Debug, PartialEq)]
struct ParsedInner {
    ast: ast::ModModule,
    errors: Vec<ParseError>,
}

impl Parsed {
    fn new(ast: ast::ModModule, errors: Vec<ParseError>) -> Self {
        Self {
            inner: Arc::new(ParsedInner { ast, errors }),
        }
    }

    pub(crate) fn from_text(text: &str) -> Self {
        let result = ruff_python_parser::parse(text, Mode::Module);

        let (module, errors) = match result {
            Ok(ast::Mod::Module(module)) => (module, vec![]),
            Ok(ast::Mod::Expression(expression)) => (
                ast::ModModule {
                    range: expression.range(),
                    body: vec![ast::Stmt::Expr(ast::StmtExpr {
                        range: expression.range(),
                        value: expression.body,
                    })],
                },
                vec![],
            ),
            Err(errors) => (
                ast::ModModule {
                    range: TextRange::default(),
                    body: Vec::new(),
                },
                vec![errors],
            ),
        };

        Parsed::new(module, errors)
    }

    pub fn ast(&self) -> &ast::ModModule {
        &self.inner.ast
    }

    pub fn errors(&self) -> &[ParseError] {
        &self.inner.errors
    }
}

#[tracing::instrument(level = "debug", skip(db))]
pub(crate) fn parse<Db>(db: &Db, file_id: FileId) -> QueryResult<Parsed>
where
    Db: SourceDb + HasJar<SourceJar>,
{
    let parsed = db.jar()?;

    parsed.parsed.get(&file_id, |file_id| {
        let source = db.source(*file_id)?;

        Ok(Parsed::from_text(source.text()))
    })
}

#[derive(Debug, Default)]
pub struct ParsedStorage(KeyValueCache<FileId, Parsed>);

impl Deref for ParsedStorage {
    type Target = KeyValueCache<FileId, Parsed>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ParsedStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
