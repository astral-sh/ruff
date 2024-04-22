use std::sync::Arc;

use crate::files::FileId;
use ruff_python_ast as ast;
use ruff_python_parser::{Mode, ParseError};
use ruff_text_size::{Ranged, TextRange};

use crate::source::Source;

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    inner: Arc<ParsedInner>,
}

#[derive(Debug, PartialEq)]
struct ParsedInner {
    file_id: FileId,
    ast: ast::ModModule,
    errors: Vec<ParseError>,
}

impl Parsed {
    fn new(file_id: FileId, ast: ast::ModModule, errors: Vec<ParseError>) -> Self {
        Self {
            inner: Arc::new(ParsedInner {
                file_id,
                ast,
                errors,
            }),
        }
    }

    pub fn ast(&self) -> &ast::ModModule {
        &self.inner.ast
    }

    pub fn file(&self) -> FileId {
        self.inner.file_id
    }

    pub fn errors(&self) -> &[ParseError] {
        &self.inner.errors
    }
}

pub(crate) fn parse(source: &Source) -> Parsed {
    let result = ruff_python_parser::parse(source.text(), Mode::Module);

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

    Parsed::new(source.file(), module, errors)
}
