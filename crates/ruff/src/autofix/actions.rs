//! Interface for generating autofix edits from higher-level actions (e.g., "remove an argument").
use anyhow::{bail, Result};
use itertools::Itertools;
use libcst_native::{
    Codegen, CodegenState, ImportNames, ParenthesizableWhitespace, SmallStatement, Statement,
};
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::ast::{self, ExcepthandlerKind, Expr, Keyword, Stmt, StmtKind};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::Edit;
use ruff_python_ast::helpers;
use ruff_python_ast::imports::{AnyImport, Import};
use ruff_python_ast::newlines::NewlineWithTrailingNewline;
use ruff_python_ast::source_code::{Indexer, Locator, Stylist};
use ruff_python_semantic::context::Context;

use crate::cst::helpers::compose_module_path;
use crate::cst::matchers::match_module;
use crate::importer::Importer;

/// Determine if a body contains only a single statement, taking into account
/// deleted.
fn has_single_child(body: &[Stmt], deleted: &[&Stmt]) -> bool {
    body.iter().filter(|child| !deleted.contains(child)).count() == 1
}

/// Determine if a child is the only statement in its body.
fn is_lone_child(child: &Stmt, parent: &Stmt, deleted: &[&Stmt]) -> Result<bool> {
    match &parent.node {
        StmtKind::FunctionDef(ast::StmtFunctionDef { body, .. })
        | StmtKind::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. })
        | StmtKind::ClassDef(ast::StmtClassDef { body, .. })
        | StmtKind::With(ast::StmtWith { body, .. })
        | StmtKind::AsyncWith(ast::StmtAsyncWith { body, .. }) => {
            if body.iter().contains(child) {
                Ok(has_single_child(body, deleted))
            } else {
                bail!("Unable to find child in parent body")
            }
        }
        StmtKind::For(ast::StmtFor { body, orelse, .. })
        | StmtKind::AsyncFor(ast::StmtAsyncFor { body, orelse, .. })
        | StmtKind::While(ast::StmtWhile { body, orelse, .. })
        | StmtKind::If(ast::StmtIf { body, orelse, .. }) => {
            if body.iter().contains(child) {
                Ok(has_single_child(body, deleted))
            } else if orelse.iter().contains(child) {
                Ok(has_single_child(orelse, deleted))
            } else {
                bail!("Unable to find child in parent body")
            }
        }
        StmtKind::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
        })
        | StmtKind::TryStar(ast::StmtTryStar {
            body,
            handlers,
            orelse,
            finalbody,
        }) => {
            if body.iter().contains(child) {
                Ok(has_single_child(body, deleted))
            } else if orelse.iter().contains(child) {
                Ok(has_single_child(orelse, deleted))
            } else if finalbody.iter().contains(child) {
                Ok(has_single_child(finalbody, deleted))
            } else if let Some(body) = handlers.iter().find_map(|handler| match &handler.node {
                ExcepthandlerKind::ExceptHandler(ast::ExcepthandlerExceptHandler {
                    body, ..
                }) => {
                    if body.iter().contains(child) {
                        Some(body)
                    } else {
                        None
                    }
                }
            }) {
                Ok(has_single_child(body, deleted))
            } else {
                bail!("Unable to find child in parent body")
            }
        }
        StmtKind::Match(ast::StmtMatch { cases, .. }) => {
            if let Some(body) = cases.iter().find_map(|case| {
                if case.body.iter().contains(child) {
                    Some(&case.body)
                } else {
                    None
                }
            }) {
                Ok(has_single_child(body, deleted))
            } else {
                bail!("Unable to find child in parent body")
            }
        }
        _ => bail!("Unable to find child in parent body"),
    }
}

/// Return the location of a trailing semicolon following a `Stmt`, if it's part
/// of a multi-statement line.
fn trailing_semicolon(stmt: &Stmt, locator: &Locator) -> Option<TextSize> {
    let contents = locator.after(stmt.end());

    for line in NewlineWithTrailingNewline::from(contents) {
        let trimmed = line.trim_start();

        if trimmed.starts_with(';') {
            let colon_offset = line.text_len() - trimmed.text_len();
            return Some(stmt.end() + line.start() + colon_offset);
        }

        if !trimmed.starts_with('\\') {
            break;
        }
    }
    None
}

/// Find the next valid break for a `Stmt` after a semicolon.
fn next_stmt_break(semicolon: TextSize, locator: &Locator) -> TextSize {
    let start_location = semicolon + TextSize::from(1);

    let contents = &locator.contents()[usize::from(start_location)..];
    for line in NewlineWithTrailingNewline::from(contents) {
        let trimmed = line.trim();
        // Skip past any continuations.
        if trimmed.starts_with('\\') {
            continue;
        }

        return start_location
            + if trimmed.is_empty() {
                // If the line is empty, then despite the previous statement ending in a
                // semicolon, we know that it's not a multi-statement line.
                line.start()
            } else {
                // Otherwise, find the start of the next statement. (Or, anything that isn't
                // whitespace.)
                let relative_offset = line.find(|c: char| !c.is_whitespace()).unwrap();
                line.start() + TextSize::try_from(relative_offset).unwrap()
            };
    }

    locator.line_end(start_location)
}

/// Return `true` if a `Stmt` occurs at the end of a file.
fn is_end_of_file(stmt: &Stmt, locator: &Locator) -> bool {
    stmt.end() == locator.contents().text_len()
}

/// Return the `Fix` to use when deleting a `Stmt`.
///
/// In some cases, this is as simple as deleting the `Range` of the `Stmt`
/// itself. However, there are a few exceptions:
/// - If the `Stmt` is _not_ the terminal statement in a multi-statement line,
///   we need to delete up to the start of the next statement (and avoid
///   deleting any content that precedes the statement).
/// - If the `Stmt` is the terminal statement in a multi-statement line, we need
///   to avoid deleting any content that precedes the statement.
/// - If the `Stmt` has no trailing and leading content, then it's convenient to
///   remove the entire start and end lines.
/// - If the `Stmt` is the last statement in its parent body, replace it with a
///   `pass` instead.
pub(crate) fn delete_stmt(
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
) -> Result<Edit> {
    if parent
        .map(|parent| is_lone_child(stmt, parent, deleted))
        .map_or(Ok(None), |v| v.map(Some))?
        .unwrap_or_default()
    {
        // If removing this node would lead to an invalid syntax tree, replace
        // it with a `pass`.
        Ok(Edit::range_replacement("pass".to_string(), stmt.range()))
    } else {
        Ok(if let Some(semicolon) = trailing_semicolon(stmt, locator) {
            let next = next_stmt_break(semicolon, locator);
            Edit::deletion(stmt.start(), next)
        } else if helpers::has_leading_content(stmt, locator) {
            Edit::range_deletion(stmt.range())
        } else if helpers::preceded_by_continuation(stmt, indexer, locator) {
            if is_end_of_file(stmt, locator) && locator.is_at_start_of_line(stmt.start()) {
                // Special-case: a file can't end in a continuation.
                Edit::range_replacement(stylist.line_ending().to_string(), stmt.range())
            } else {
                Edit::range_deletion(stmt.range())
            }
        } else {
            let range = locator.full_lines_range(stmt.range());
            Edit::range_deletion(range)
        })
    }
}

/// Generate a `Fix` to remove any unused imports from an `import` statement.
pub(crate) fn remove_unused_imports<'a>(
    unused_imports: impl Iterator<Item = &'a str>,
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(stmt.range());
    let mut tree = match_module(module_text)?;

    let Some(Statement::Simple(body)) = tree.body.first_mut() else {
        bail!("Expected Statement::Simple");
    };

    let (aliases, import_module) = match body.body.first_mut() {
        Some(SmallStatement::Import(import_body)) => (&mut import_body.names, None),
        Some(SmallStatement::ImportFrom(import_body)) => {
            if let ImportNames::Aliases(names) = &mut import_body.names {
                (
                    names,
                    Some((&import_body.relative, import_body.module.as_ref())),
                )
            } else if let ImportNames::Star(..) = &import_body.names {
                // Special-case: if the import is a `from ... import *`, then we delete the
                // entire statement.
                let mut found_star = false;
                for unused_import in unused_imports {
                    let full_name = match import_body.module.as_ref() {
                        Some(module_name) => format!("{}.*", compose_module_path(module_name)),
                        None => "*".to_string(),
                    };
                    if unused_import == full_name {
                        found_star = true;
                    } else {
                        bail!(
                            "Expected \"*\" for unused import (got: \"{}\")",
                            unused_import
                        );
                    }
                }
                if !found_star {
                    bail!("Expected \'*\' for unused import");
                }
                return delete_stmt(stmt, parent, deleted, locator, indexer, stylist);
            } else {
                bail!("Expected: ImportNames::Aliases | ImportNames::Star");
            }
        }
        _ => bail!("Expected: SmallStatement::ImportFrom | SmallStatement::Import"),
    };

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    for unused_import in unused_imports {
        let alias_index = aliases.iter().position(|alias| {
            let full_name = match import_module {
                Some((relative, module)) => {
                    let module = module.map(compose_module_path);
                    let member = compose_module_path(&alias.name);
                    let mut full_name = String::with_capacity(
                        relative.len()
                            + module.as_ref().map_or(0, std::string::String::len)
                            + member.len()
                            + 1,
                    );
                    for _ in 0..relative.len() {
                        full_name.push('.');
                    }
                    if let Some(module) = module {
                        full_name.push_str(&module);
                        full_name.push('.');
                    }
                    full_name.push_str(&member);
                    full_name
                }
                None => compose_module_path(&alias.name),
            };
            full_name == unused_import
        });

        if let Some(index) = alias_index {
            aliases.remove(index);
        }
    }

    // But avoid destroying any trailing comments.
    if let Some(alias) = aliases.last_mut() {
        let has_comment = if let Some(comma) = &alias.comma {
            match &comma.whitespace_after {
                ParenthesizableWhitespace::SimpleWhitespace(_) => false,
                ParenthesizableWhitespace::ParenthesizedWhitespace(whitespace) => {
                    whitespace.first_line.comment.is_some()
                }
            }
        } else {
            false
        };
        if !has_comment {
            alias.comma = trailing_comma;
        }
    }

    if aliases.is_empty() {
        delete_stmt(stmt, parent, deleted, locator, indexer, stylist)
    } else {
        let mut state = CodegenState {
            default_newline: &stylist.line_ending(),
            default_indent: stylist.indentation(),
            ..CodegenState::default()
        };
        tree.codegen(&mut state);

        Ok(Edit::range_replacement(state.to_string(), stmt.range()))
    }
}

/// Generic function to remove arguments or keyword arguments in function
/// calls and class definitions. (For classes `args` should be considered
/// `bases`)
///
/// Supports the removal of parentheses when this is the only (kw)arg left.
/// For this behavior, set `remove_parentheses` to `true`.
pub(crate) fn remove_argument(
    locator: &Locator,
    call_at: TextSize,
    expr_range: TextRange,
    args: &[Expr],
    keywords: &[Keyword],
    remove_parentheses: bool,
) -> Result<Edit> {
    // TODO(sbrugman): Preserve trailing comments.
    let contents = locator.after(call_at);

    let mut fix_start = None;
    let mut fix_end = None;

    let n_arguments = keywords.len() + args.len();
    if n_arguments == 0 {
        bail!("No arguments or keywords to remove");
    }

    if n_arguments == 1 {
        // Case 1: there is only one argument.
        let mut count: usize = 0;
        for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, call_at).flatten() {
            if matches!(tok, Tok::Lpar) {
                if count == 0 {
                    fix_start = Some(if remove_parentheses {
                        range.start()
                    } else {
                        range.start() + TextSize::from(1)
                    });
                }
                count += 1;
            }

            if matches!(tok, Tok::Rpar) {
                count -= 1;
                if count == 0 {
                    fix_end = Some(if remove_parentheses {
                        range.end()
                    } else {
                        range.end() - TextSize::from(1)
                    });
                    break;
                }
            }
        }
    } else if args
        .iter()
        .map(Expr::start)
        .chain(keywords.iter().map(Keyword::start))
        .any(|location| location > expr_range.start())
    {
        // Case 2: argument or keyword is _not_ the last node.
        let mut seen_comma = false;
        for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, call_at).flatten() {
            if seen_comma {
                if matches!(tok, Tok::NonLogicalNewline) {
                    // Also delete any non-logical newlines after the comma.
                    continue;
                }
                fix_end = Some(if matches!(tok, Tok::Newline) {
                    range.end()
                } else {
                    range.start()
                });
                break;
            }
            if range.start() == expr_range.start() {
                fix_start = Some(range.start());
            }
            if fix_start.is_some() && matches!(tok, Tok::Comma) {
                seen_comma = true;
            }
        }
    } else {
        // Case 3: argument or keyword is the last node, so we have to find the last
        // comma in the stmt.
        for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, call_at).flatten() {
            if range.start() == expr_range.start() {
                fix_end = Some(expr_range.end());
                break;
            }
            if matches!(tok, Tok::Comma) {
                fix_start = Some(range.start());
            }
        }
    }

    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Ok(Edit::deletion(start, end)),
        _ => {
            bail!("No fix could be constructed")
        }
    }
}

/// Generate an [`Edit`] to reference the given symbol. Returns the [`Edit`] necessary to make the
/// symbol available in the current scope along with the bound name of the symbol.
///
/// For example, assuming `module` is `"functools"` and `member` is `"lru_cache"`, this function
/// could return an [`Edit`] to add `import functools` to the top of the file, alongside with the
/// name on which the `lru_cache` symbol would be made available (`"functools.lru_cache"`).
///
/// Attempts to reuse existing imports when possible.
pub(crate) fn get_or_import_symbol(
    module: &str,
    member: &str,
    at: TextSize,
    context: &Context,
    importer: &Importer,
    locator: &Locator,
) -> Result<(Edit, String)> {
    if let Some((source, binding)) = context.resolve_qualified_import_name(module, member) {
        // If the symbol is already available in the current scope, use it.

        // The exception: the symbol source (i.e., the import statement) comes after the current
        // location. For example, we could be generating an edit within a function, and the import
        // could be defined in the module scope, but after the function definition. In this case,
        // it's unclear whether we can use the symbol (the function could be called between the
        // import and the current location, and thus the symbol would not be available). It's also
        // unclear whether should add an import statement at the top of the file, since it could
        // be shadowed between the import and the current location.
        if source.start() > at {
            bail!("Unable to use existing symbol `{binding}` due to late-import");
        }

        // We also add a no-op edit to force conflicts with any other fixes that might try to
        // remove the import. Consider:
        //
        // ```py
        // import sys
        //
        // quit()
        // ```
        //
        // Assume you omit this no-op edit. If you run Ruff with `unused-imports` and
        // `sys-exit-alias` over this snippet, it will generate two fixes: (1) remove the unused
        // `sys` import; and (2) replace `quit()` with `sys.exit()`, under the assumption that `sys`
        // is already imported and available.
        //
        // By adding this no-op edit, we force the `unused-imports` fix to conflict with the
        // `sys-exit-alias` fix, and thus will avoid applying both fixes in the same pass.
        let import_edit =
            Edit::range_replacement(locator.slice(source.range()).to_string(), source.range());
        Ok((import_edit, binding))
    } else {
        if let Some(stmt) = importer.find_import_from(module, at) {
            // Case 1: `from functools import lru_cache` is in scope, and we're trying to reference
            // `functools.cache`; thus, we add `cache` to the import, and return `"cache"` as the
            // bound name.
            if context
                .find_binding(member)
                .map_or(true, |binding| binding.kind.is_builtin())
            {
                let import_edit = importer.add_member(stmt, member)?;
                Ok((import_edit, member.to_string()))
            } else {
                bail!("Unable to insert `{member}` into scope due to name conflict")
            }
        } else {
            // Case 2: No `functools` import is in scope; thus, we add `import functools`, and
            // return `"functools.cache"` as the bound name.
            if context
                .find_binding(module)
                .map_or(true, |binding| binding.kind.is_builtin())
            {
                let import_edit =
                    importer.add_import(&AnyImport::Import(Import::module(module)), at);
                Ok((import_edit, format!("{module}.{member}")))
            } else {
                bail!("Unable to insert `{module}` into scope due to name conflict")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ruff_text_size::TextSize;
    use rustpython_parser as parser;

    use ruff_python_ast::source_code::Locator;

    use crate::autofix::actions::{next_stmt_break, trailing_semicolon};

    #[test]
    fn find_semicolon() -> Result<()> {
        let contents = "x = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(trailing_semicolon(stmt, &locator), None);

        let contents = "x = 1; y = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(trailing_semicolon(stmt, &locator), Some(TextSize::from(5)));

        let contents = "x = 1 ; y = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(trailing_semicolon(stmt, &locator), Some(TextSize::from(6)));

        let contents = r#"
x = 1 \
  ; y = 1
"#
        .trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(trailing_semicolon(stmt, &locator), Some(TextSize::from(10)));

        Ok(())
    }

    #[test]
    fn find_next_stmt_break() {
        let contents = "x = 1; y = 1";
        let locator = Locator::new(contents);
        assert_eq!(
            next_stmt_break(TextSize::from(4), &locator),
            TextSize::from(5)
        );

        let contents = "x = 1 ; y = 1";
        let locator = Locator::new(contents);
        assert_eq!(
            next_stmt_break(TextSize::from(5), &locator),
            TextSize::from(6)
        );

        let contents = r#"
x = 1 \
  ; y = 1
"#
        .trim();
        let locator = Locator::new(contents);
        assert_eq!(
            next_stmt_break(TextSize::from(10), &locator),
            TextSize::from(12)
        );
    }
}
