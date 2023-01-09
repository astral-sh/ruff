use std::fmt;

use log::error;
use rustpython_ast::{Location, StmtKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::isort::track::Block;
use crate::registry::{Diagnostic, RuleCode};
use crate::settings::{flags, Settings};
use crate::violations;

struct Alias<'a> {
    name: &'a str,
    as_name: Option<&'a str>,
}

struct ImportFrom<'a> {
    module: Option<&'a str>,
    name: Alias<'a>,
    level: Option<&'a usize>,
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
        write!(f, " import {}", self.name.name)
    }
}

fn has_required_import(block: &Block, required_import: &ImportFrom) -> bool {
    block.imports.iter().any(|import| {
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
    })
}

/// Find the first token that isn't a docstring, comment, or whitespace.
fn find_splice_location(contents: &str) -> Location {
    let mut splice = Location::default();
    for (.., tok, end) in lexer::make_tokenizer(contents).flatten() {
        if matches!(tok, Tok::String { .. } | Tok::Comment(..) | Tok::Newline) {
            splice = end;
        } else {
            break;
        }
    }
    splice
}

fn add_required_import(
    required_import: &ImportFrom,
    contents: &str,
    blocks: &[&Block],
    settings: &Settings,
    autofix: flags::Autofix,
) -> Option<Diagnostic> {
    // If the import is already present in a top-level block, don't add it.
    if blocks
        .iter()
        .filter(|block| !block.nested)
        .any(|block| has_required_import(block, required_import))
    {
        return None;
    }

    // Always insert the diagnostic at top-of-file.
    let mut diagnostic = Diagnostic::new(
        violations::MissingRequiredImport(required_import.to_string()),
        Range::new(Location::default(), Location::default()),
    );
    if matches!(autofix, flags::Autofix::Enabled) && settings.fixable.contains(&RuleCode::I002) {
        // Determine the location at which the import should be inserted.
        let splice = find_splice_location(contents);

        // Generate the edit. If we're inserting beyond the start of the file, we need
        // to add a newline, since the splice represents the _end_ of the last
        // irrelevant token (e.g., the end of a comment or the end of
        // docstring). This ensures that we properly handle awkward cases like
        // docstrings that are followed by semicolons.
        let mut contents = String::new();
        if splice > Location::default() {
            contents.push('\n');
        }
        contents.push_str(&required_import.to_string());
        contents.push('\n');

        // Construct the fix.
        diagnostic.amend(Fix::insertion(contents, splice));
    }
    Some(diagnostic)
}

/// I002
pub fn add_required_imports(
    contents: &str,
    blocks: &[&Block],
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
            let StmtKind::ImportFrom { module, names, level } = &body[0].node else {
                error!("Expected required import to be in import-from style: `{}`", required_import);
                return vec![];
            };
            names.iter().filter_map(|name| {
                add_required_import(
                    &ImportFrom {
                        module: module.as_ref().map(String::as_str),
                        name: Alias {
                            name: name.node.name.as_str(),
                            as_name: name.node.asname.as_deref(),
                        },
                        level: level.as_ref(),
                    },
                    contents,
                    blocks,
                    settings,
                    autofix,
                )
            }).collect()
        })
        .collect()
}
