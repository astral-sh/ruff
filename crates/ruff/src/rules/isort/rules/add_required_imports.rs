use std::fmt;

use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Location, StmtKind, Suite};

use super::super::helpers;
use super::super::track::Block;
use crate::ast::helpers::is_docstring_stmt;
use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::settings::{flags, Settings};
use crate::source_code::{Locator, Stylist};
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct MissingRequiredImport(pub String);
);
impl AlwaysAutofixableViolation for MissingRequiredImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingRequiredImport(name) = self;
        format!("Missing required import: `{name}`")
    }

    fn autofix_title(&self) -> String {
        let MissingRequiredImport(name) = self;
        format!("Insert required import: `{name}`")
    }
}

struct Alias<'a> {
    name: &'a str,
    as_name: Option<&'a str>,
}

struct ImportFrom<'a> {
    module: Option<&'a str>,
    name: Alias<'a>,
    level: Option<&'a usize>,
}

struct Import<'a> {
    name: Alias<'a>,
}

enum AnyImport<'a> {
    Import(Import<'a>),
    ImportFrom(ImportFrom<'a>),
}

impl fmt::Display for ImportFrom<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "from ")?;
        if let Some(level) = self.level {
            write!(f, "{}", ".".repeat(*level))?;
        }
        if let Some(module) = self.module {
            write!(f, "{module}")?;
        }
        write!(f, " import {}", self.name.name)?;
        Ok(())
    }
}

impl fmt::Display for Import<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "import {}", self.name.name)?;
        if let Some(as_name) = self.name.as_name {
            write!(f, " as {as_name}")?;
        }
        Ok(())
    }
}

impl fmt::Display for AnyImport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AnyImport::Import(import) => write!(f, "{import}"),
            AnyImport::ImportFrom(import_from) => write!(f, "{import_from}"),
        }
    }
}

fn contains(block: &Block, required_import: &AnyImport) -> bool {
    block.imports.iter().any(|import| match required_import {
        AnyImport::Import(required_import) => {
            let StmtKind::Import {
                names,
            } = &import.node else {
                return false;
            };
            names.iter().any(|alias| {
                alias.node.name == required_import.name.name
                    && alias.node.asname.as_deref() == required_import.name.as_name
            })
        }
        AnyImport::ImportFrom(required_import) => {
            let StmtKind::ImportFrom {
                module,
                names,
                level,
            } = &import.node else {
                return false;
            };
            module.as_deref() == required_import.module
                && level.as_ref() == required_import.level
                && names.iter().any(|alias| {
                    alias.node.name == required_import.name.name
                        && alias.node.asname.as_deref() == required_import.name.as_name
                })
        }
    })
}

fn add_required_import(
    required_import: &AnyImport,
    blocks: &[&Block],
    python_ast: &Suite,
    locator: &Locator,
    stylist: &Stylist,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Option<Diagnostic> {
    // If the import is already present in a top-level block, don't add it.
    if blocks
        .iter()
        .filter(|block| !block.nested)
        .any(|block| contains(block, required_import))
    {
        return None;
    }

    // Don't add imports to semantically-empty files.
    if python_ast.iter().all(is_docstring_stmt) {
        return None;
    }

    // Always insert the diagnostic at top-of-file.
    let required_import = required_import.to_string();
    let mut diagnostic = Diagnostic::new(
        MissingRequiredImport(required_import.clone()),
        Range::new(Location::default(), Location::default()),
    );
    if matches!(autofix, flags::Autofix::Enabled)
        && settings.rules.should_fix(&Rule::MissingRequiredImport)
    {
        // Determine the location at which the import should be inserted.
        let splice = helpers::find_splice_location(python_ast, locator);

        // Generate the edit.
        let mut contents = String::with_capacity(required_import.len() + 1);

        // Newline (LF/CRLF)
        let line_sep = stylist.line_ending().as_str();

        // If we're inserting beyond the start of the file, we add
        // a newline _before_, since the splice represents the _end_ of the last
        // irrelevant token (e.g., the end of a comment or the end of
        // docstring). This ensures that we properly handle awkward cases like
        // docstrings that are followed by semicolons.
        if splice > Location::default() {
            contents.push_str(line_sep);
        }
        contents.push_str(&required_import);

        // If we're inserting at the start of the file, add a trailing newline instead.
        if splice == Location::default() {
            contents.push_str(line_sep);
        }

        // Construct the fix.
        diagnostic.amend(Fix::insertion(contents, splice));
    }
    Some(diagnostic)
}

/// I002
pub fn add_required_imports(
    blocks: &[&Block],
    python_ast: &Suite,
    locator: &Locator,
    stylist: &Stylist,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Diagnostic> {
    settings
        .isort
        .required_imports
        .iter()
        .flat_map(|required_import| {
            let Ok(body) = rustpython_parser::parser::parse_program(required_import, "<filename>") else {
                error!("Failed to parse required import: `{}`", required_import);
                return vec![];
            };
            if body.is_empty() || body.len() > 1 {
                error!("Expected require import to contain a single statement: `{}`", required_import);
                return vec![];
            }

            match &body[0].node {
                StmtKind::ImportFrom { module, names, level } => {
                    names.iter().filter_map(|name| {
                        add_required_import(
                            &AnyImport::ImportFrom(ImportFrom {
                                module: module.as_ref().map(String::as_str),
                                name: Alias {
                                    name: name.node.name.as_str(),
                                    as_name: name.node.asname.as_deref(),
                                },
                                level: level.as_ref(),
                            }),
                            blocks,
                            python_ast,
                            locator,
                            stylist,
                            settings,
                            autofix,
                        )
                    }).collect()
                }
                StmtKind::Import { names } => {
                    names.iter().filter_map(|name| {
                        add_required_import(
                            &AnyImport::Import(Import {
                                name: Alias {
                                    name: name.node.name.as_str(),
                                    as_name: name.node.asname.as_deref(),
                                },
                            }),
                            blocks,
                            python_ast,
                            locator,
                            stylist,
                            settings,
                            autofix,
                        )
                    }).collect()
                }
                _ => {
                    error!("Expected required import to be in import-from style: `{}`", required_import);
                    vec![]
                }
            }
        })
        .collect()
}
