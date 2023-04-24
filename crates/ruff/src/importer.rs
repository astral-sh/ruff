//! Add and modify import statements to make module members available during fix execution.

use anyhow::Result;
use libcst_native::{Codegen, CodegenState, ImportAlias, Name, NameOrAttribute};
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Location, Stmt, StmtKind, Suite};
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
    /// A map from module name to top-level `StmtKind::ImportFrom` statements.
    import_from_map: FxHashMap<&'a str, &'a Stmt>,
    /// The last top-level import statement.
    trailing_import: Option<&'a Stmt>,
}

impl<'a> Importer<'a> {
    pub fn new(python_ast: &'a Suite, locator: &'a Locator<'a>, stylist: &'a Stylist<'a>) -> Self {
        Self {
            python_ast,
            locator,
            stylist,
            import_from_map: FxHashMap::default(),
            trailing_import: None,
        }
    }

    /// Visit a top-level import statement.
    pub fn visit_import(&mut self, import: &'a Stmt) {
        // Store a reference to the import statement in the appropriate map.
        match &import.node {
            StmtKind::Import { .. } => {
                // Nothing to do here, we don't extend top-level `import` statements at all, so
                // no need to track them.
            }
            StmtKind::ImportFrom { module, level, .. } => {
                // Store a reverse-map from module name to `import ... from` statement.
                if level.map_or(true, |level| level == 0) {
                    if let Some(module) = module {
                        self.import_from_map.insert(module.as_str(), import);
                    }
                }
            }
            _ => {
                panic!("Expected StmtKind::Import | StmtKind::ImportFrom");
            }
        }

        // Store a reference to the last top-level import statement.
        self.trailing_import = Some(import);
    }

    /// Add an import statement to import the given module.
    ///
    /// If there are no existing imports, the new import will be added at the top
    /// of the file. Otherwise, it will be added after the most recent top-level
    /// import statement.
    pub fn add_import(&self, import: &AnyImport) -> Edit {
        let required_import = import.to_string();
        if let Some(stmt) = self.trailing_import {
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

    /// Return the top-level [`Stmt`] that imports the given module using `StmtKind::ImportFrom`.
    /// if it exists.
    pub fn get_import_from(&self, module: &str) -> Option<&Stmt> {
        self.import_from_map.get(module).copied()
    }

    /// Add the given member to an existing `StmtKind::ImportFrom` statement.
    pub fn add_member(&self, stmt: &Stmt, member: &str) -> Result<Edit> {
        let mut tree = match_module(self.locator.slice(stmt))?;
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
        Ok(Edit::replacement(
            state.to_string(),
            stmt.location,
            stmt.end_location.unwrap(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Insertion {
    /// The content to add before the insertion.
    prefix: &'static str,
    /// The location at which to insert.
    location: Location,
    /// The content to add after the insertion.
    suffix: &'static str,
}

impl Insertion {
    fn new(prefix: &'static str, location: Location, suffix: &'static str) -> Self {
        Self {
            prefix,
            location,
            suffix,
        }
    }
}

/// Find the end of the last docstring.
fn match_docstring_end(body: &[Stmt]) -> Option<Location> {
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
    Some(stmt.end_location.unwrap())
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
fn end_of_statement_insertion(stmt: &Stmt, locator: &Locator, stylist: &Stylist) -> Insertion {
    let location = stmt.end_location.unwrap();
    let mut tokens = lexer::lex_located(locator.after(location), Mode::Module, location).flatten();
    if let Some((.., Tok::Semi, end)) = tokens.next() {
        // If the first token after the docstring is a semicolon, insert after the semicolon as an
        // inline statement;
        Insertion::new(" ", end, ";")
    } else {
        // Otherwise, insert on the next line.
        Insertion::new(
            "",
            Location::new(location.row() + 1, 0),
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
        let first_token = lexer::lex_located(locator.after(location), Mode::Module, location)
            .flatten()
            .next();
        if let Some((.., Tok::Semi, end)) = first_token {
            return Insertion::new(" ", end, ";");
        }

        // Otherwise, advance to the next row.
        Location::new(location.row() + 1, 0)
    } else {
        Location::default()
    };

    // Skip over any comments and empty lines.
    for (.., tok, end) in
        lexer::lex_located(locator.after(location), Mode::Module, location).flatten()
    {
        if matches!(tok, Tok::Comment(..) | Tok::Newline) {
            location = Location::new(end.row() + 1, 0);
        } else {
            break;
        }
    }

    return Insertion::new("", location, stylist.line_ending().as_str());
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser as parser;
    use rustpython_parser::ast::Location;
    use rustpython_parser::lexer::LexResult;

    use ruff_python_ast::source_code::{LineEnding, Locator, Stylist};

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
            Insertion::new("", Location::new(1, 0), LineEnding::default().as_str())
        );

        let contents = r#"
"""Hello, world!""""#
            .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(2, 0), LineEnding::default().as_str())
        );

        let contents = r#"
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(2, 0), "\n")
        );

        let contents = r#"
"""Hello, world!"""
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(3, 0), "\n")
        );

        let contents = r#"
x = 1
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(1, 0), "\n")
        );

        let contents = r#"
#!/usr/bin/env python3
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(2, 0), "\n")
        );

        let contents = r#"
#!/usr/bin/env python3
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(3, 0), "\n")
        );

        let contents = r#"
"""Hello, world!"""
#!/usr/bin/env python3
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(3, 0), "\n")
        );

        let contents = r#"
"""%s""" % "Hello, world!"
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(1, 0), "\n")
        );

        let contents = r#"
"""Hello, world!"""; x = 1
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new(" ", Location::new(1, 20), ";")
        );

        let contents = r#"
"""Hello, world!"""; x = 1; y = \
    2
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new(" ", Location::new(1, 20), ";")
        );

        Ok(())
    }
}
