use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::StringLiteral;

use crate::cache::KeyValueCache;
use crate::db::{HasJar, SourceDb, SourceJar};
use crate::files::FileId;

pub(crate) fn lint_syntax<Db>(db: &Db, file_id: FileId) -> Diagnostics
where
    Db: SourceDb + HasJar<SourceJar>,
{
    let storage = &db.jar().lint_syntax;

    storage.get(&file_id, |file_id| {
        let mut diagnostics = Vec::new();

        let source = db.source(*file_id);
        lint_lines(source.text(), &mut diagnostics);

        let parsed = db.parse(*file_id);

        if parsed.errors().is_empty() {
            let ast = parsed.ast();

            let mut visitor = SyntaxLintVisitor {
                diagnostics,
                source: source.text(),
            };
            visitor.visit_body(&ast.body);
            Diagnostics::from(visitor.diagnostics)
        } else {
            Diagnostics::Empty
        }
    })
}

pub(crate) fn lint_lines(source: &str, diagnostics: &mut Vec<String>) {
    for (line_number, line) in source.lines().enumerate() {
        if line.len() < 88 {
            continue;
        }

        let char_count = line.chars().count();
        if char_count > 88 {
            diagnostics.push(format!(
                "Line {} is too long ({} characters)",
                line_number + 1,
                char_count
            ));
        }
    }
}

#[derive(Debug)]
struct SyntaxLintVisitor<'a> {
    diagnostics: Vec<String>,
    source: &'a str,
}

impl Visitor<'_> for SyntaxLintVisitor<'_> {
    fn visit_string_literal(&mut self, string_literal: &'_ StringLiteral) {
        // A very naive implementation of use double quotes
        let text = &self.source[string_literal.range];

        if text.starts_with('\'') {
            self.diagnostics
                .push("Use double quotes for strings".to_string());
        }
    }
}

#[derive(Debug, Clone)]
pub enum Diagnostics {
    Empty,
    List(Arc<Vec<String>>),
}

impl Diagnostics {
    pub fn as_slice(&self) -> &[String] {
        match self {
            Diagnostics::Empty => &[],
            Diagnostics::List(list) => list.as_slice(),
        }
    }
}

impl Deref for Diagnostics {
    type Target = [String];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl From<Vec<String>> for Diagnostics {
    fn from(value: Vec<String>) -> Self {
        if value.is_empty() {
            Diagnostics::Empty
        } else {
            Diagnostics::List(Arc::new(value))
        }
    }
}

#[derive(Default, Debug)]
pub struct LintSyntaxStorage(KeyValueCache<FileId, Diagnostics>);

impl Deref for LintSyntaxStorage {
    type Target = KeyValueCache<FileId, Diagnostics>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LintSyntaxStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
