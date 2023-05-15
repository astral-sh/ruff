//! Add and modify import statements to make module members available during fix execution.

use anyhow::Result;
use libcst_native::{Codegen, CodegenState, ImportAlias, Name, NameOrAttribute};
use ruff_text_size::TextSize;
use rustpython_parser::ast::{self, Stmt, StmtKind, Suite};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::Edit;
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::imports::AnyImport;
use ruff_python_ast::source_code::{Locator, Stylist};

use crate::cst::matchers::{match_aliases, match_import_from, match_module};

pub struct Importer<'a> {
    python_ast: &'a Suite,
    locator: &'a Locator<'a>,
    stylist: &'a Stylist<'a>,
    ordered_imports: Vec<&'a Stmt>,
}

impl<'a> Importer<'a> {
    pub fn new(python_ast: &'a Suite, locator: &'a Locator<'a>, stylist: &'a Stylist<'a>) -> Self {
        Self {
            python_ast,
            locator,
            stylist,
            ordered_imports: Vec::default(),
        }
    }

    /// Visit a top-level import statement.
    pub fn visit_import(&mut self, import: &'a Stmt) {
        self.ordered_imports.push(import);
    }

    /// Return the import statement that precedes the given position, if any.
    fn preceding_import(&self, at: TextSize) -> Option<&Stmt> {
        self.ordered_imports
            .partition_point(|stmt| stmt.start() < at)
            .checked_sub(1)
            .map(|idx| self.ordered_imports[idx])
    }

    /// Add an import statement to import the given module.
    ///
    /// If there are no existing imports, the new import will be added at the top
    /// of the file. Otherwise, it will be added after the most recent top-level
    /// import statement.
    pub fn add_import(&self, import: &AnyImport, at: TextSize) -> Edit {
        let required_import = import.to_string();
        if let Some(stmt) = self.preceding_import(at) {
            // Insert after the last top-level import.
            let Insertion {
                prefix,
                location,
                suffix,
            } = end_of_statement_insertion(stmt, self.locator, self.stylist);
            let content = format!("{prefix}{required_import}{suffix}");
            Edit::insertion(content, location)
        } else {
            // Insert at the top of the file.
            let Insertion {
                prefix,
                location,
                suffix,
            } = top_of_file_insertion(self.python_ast, self.locator, self.stylist);
            let content = format!("{prefix}{required_import}{suffix}");
            Edit::insertion(content, location)
        }
    }

    /// Return the top-level [`Stmt`] that imports the given module using `StmtKind::ImportFrom`
    /// preceding the given position, if any.
    pub fn find_import_from(&self, module: &str, at: TextSize) -> Option<&Stmt> {
        let mut import_from = None;
        for stmt in &self.ordered_imports {
            if stmt.start() >= at {
                break;
            }
            if let StmtKind::ImportFrom(ast::StmtImportFrom {
                module: name,
                level,
                ..
            }) = &stmt.node
            {
                if level.map_or(true, |level| level.to_u32() == 0)
                    && name.as_ref().map_or(false, |name| name == module)
                {
                    import_from = Some(*stmt);
                }
            }
        }
        import_from
    }

    /// Add the given member to an existing `StmtKind::ImportFrom` statement.
    pub fn add_member(&self, stmt: &Stmt, member: &str) -> Result<Edit> {
        let mut tree = match_module(self.locator.slice(stmt.range()))?;
        let import_from = match_import_from(&mut tree)?;
        let aliases = match_aliases(import_from)?;
        aliases.push(ImportAlias {
            name: NameOrAttribute::N(Box::new(Name {
                value: member,
                lpar: vec![],
                rpar: vec![],
            })),
            asname: None,
            comma: aliases.last().and_then(|alias| alias.comma.clone()),
        });
        let mut state = CodegenState {
            default_newline: &self.stylist.line_ending(),
            default_indent: self.stylist.indentation(),
            ..CodegenState::default()
        };
        tree.codegen(&mut state);
        Ok(Edit::range_replacement(state.to_string(), stmt.range()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Insertion {
    /// The content to add before the insertion.
    prefix: &'static str,
    /// The location at which to insert.
    location: TextSize,
    /// The content to add after the insertion.
    suffix: &'static str,
}

impl Insertion {
    fn new(prefix: &'static str, location: TextSize, suffix: &'static str) -> Self {
        Self {
            prefix,
            location,
            suffix,
        }
    }
}

/// Find the end of the last docstring.
fn match_docstring_end(body: &[Stmt]) -> Option<TextSize> {
    let mut iter = body.iter();
    let Some(mut stmt) = iter.next() else {
        return None;
    };
    if !is_docstring_stmt(stmt) {
        return None;
    }
    for next in iter {
        if !is_docstring_stmt(next) {
            break;
        }
        stmt = next;
    }
    Some(stmt.end())
}

/// Find the location at which an "end-of-statement" import should be inserted,
/// along with a prefix and suffix to use for the insertion.
///
/// For example, given the following code:
///
/// ```python
/// """Hello, world!"""
///
/// import os
/// import math
///
///
/// def foo():
///     pass
/// ```
///
/// The location returned will be the start of new line after the last
/// import statement, which in this case is the line after `import math`,
/// along with a trailing newline suffix.
fn end_of_statement_insertion(stmt: &Stmt, locator: &Locator, stylist: &Stylist) -> Insertion {
    let location = stmt.end();
    let mut tokens =
        lexer::lex_starts_at(locator.after(location), Mode::Module, location).flatten();
    if let Some((Tok::Semi, range)) = tokens.next() {
        // If the first token after the docstring is a semicolon, insert after the semicolon as an
        // inline statement;
        Insertion::new(" ", range.end(), ";")
    } else {
        // Otherwise, insert on the next line.
        Insertion::new(
            "",
            locator.full_line_end(location),
            stylist.line_ending().as_str(),
        )
    }
}

/// Find the location at which a "top-of-file" import should be inserted,
/// along with a prefix and suffix to use for the insertion.
///
/// For example, given the following code:
///
/// ```python
/// """Hello, world!"""
///
/// import os
/// ```
///
/// The location returned will be the start of the `import os` statement,
/// along with a trailing newline suffix.
fn top_of_file_insertion(body: &[Stmt], locator: &Locator, stylist: &Stylist) -> Insertion {
    // Skip over any docstrings.
    let mut location = if let Some(location) = match_docstring_end(body) {
        // If the first token after the docstring is a semicolon, insert after the semicolon as an
        // inline statement;
        let first_token = lexer::lex_starts_at(locator.after(location), Mode::Module, location)
            .flatten()
            .next();
        if let Some((Tok::Semi, range)) = first_token {
            return Insertion::new(" ", range.end(), ";");
        }

        // Otherwise, advance to the next row.
        locator.full_line_end(location)
    } else {
        TextSize::default()
    };

    // Skip over any comments and empty lines.
    for (tok, range) in
        lexer::lex_starts_at(locator.after(location), Mode::Module, location).flatten()
    {
        if matches!(tok, Tok::Comment(..) | Tok::Newline) {
            location = locator.full_line_end(range.end());
        } else {
            break;
        }
    }

    return Insertion::new("", location, stylist.line_ending().as_str());
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ruff_text_size::TextSize;
    use rustpython_parser as parser;
    use rustpython_parser::lexer::LexResult;

    use ruff_python_ast::newlines::LineEnding;
    use ruff_python_ast::source_code::{Locator, Stylist};

    use crate::importer::{top_of_file_insertion, Insertion};

    fn insert(contents: &str) -> Result<Insertion> {
        let program = parser::parse_program(contents, "<filename>")?;
        let tokens: Vec<LexResult> = ruff_rustpython::tokenize(contents);
        let locator = Locator::new(contents);
        let stylist = Stylist::from_tokens(&tokens, &locator);
        Ok(top_of_file_insertion(&program, &locator, &stylist))
    }

    #[test]
    fn top_of_file_insertions() -> Result<()> {
        let contents = "";
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(0), LineEnding::default().as_str())
        );

        let contents = r#"
"""Hello, world!""""#
            .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(19), LineEnding::default().as_str())
        );

        let contents = r#"
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(20), "\n")
        );

        let contents = r#"
"""Hello, world!"""
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(40), "\n")
        );

        let contents = r#"
x = 1
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(0), "\n")
        );

        let contents = r#"
#!/usr/bin/env python3
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(23), "\n")
        );

        let contents = r#"
#!/usr/bin/env python3
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(43), "\n")
        );

        let contents = r#"
"""Hello, world!"""
#!/usr/bin/env python3
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(43), "\n")
        );

        let contents = r#"
"""%s""" % "Hello, world!"
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(0), "\n")
        );

        let contents = r#"
"""Hello, world!"""; x = 1
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new(" ", TextSize::from(20), ";")
        );

        let contents = r#"
"""Hello, world!"""; x = 1; y = \
    2
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new(" ", TextSize::from(20), ";")
        );

        Ok(())
    }
}
