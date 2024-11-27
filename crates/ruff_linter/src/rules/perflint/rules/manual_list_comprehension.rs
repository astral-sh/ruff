use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};

use anyhow::{anyhow, Result};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_semantic::{analyze::typing::is_list, Binding};
use ruff_python_trivia::PythonWhitespace;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `for` loops that can be replaced by a list comprehension.
///
/// ## Why is this bad?
/// When creating a transformed list from an existing list using a for-loop,
/// prefer a list comprehension. List comprehensions are more readable and
/// more performant.
///
/// Using the below as an example, the list comprehension is ~10% faster on
/// Python 3.11, and ~25% faster on Python 3.10.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// original = list(range(10000))
/// filtered = []
/// for i in original:
///     if i % 2:
///         filtered.append(i)
/// ```
///
/// Use instead:
/// ```python
/// original = list(range(10000))
/// filtered = [x for x in original if x % 2]
/// ```
///
/// If you're appending to an existing list, use the `extend` method instead:
/// ```python
/// original = list(range(10000))
/// filtered.extend(x for x in original if x % 2)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ManualListComprehension {
    is_async: bool,
    comprehension_type: Option<ComprehensionType>,
}

impl Violation for ManualListComprehension {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ManualListComprehension {
            is_async,
            comprehension_type,
        } = self;
        let message_str = match comprehension_type {
            Some(ComprehensionType::Extend) => {
                if *is_async {
                    "`list.extend` with an async comprehension"
                } else {
                    "`list.extend`"
                }
            }
            Some(ComprehensionType::ListComprehension) | None => {
                if *is_async {
                    "an async list comprehension"
                } else {
                    "a list comprehension"
                }
            }
        };
        format!("Use {message_str} to create a transformed list")
    }

    fn fix_title(&self) -> Option<String> {
        match self.comprehension_type? {
            ComprehensionType::ListComprehension => {
                Some("Replace for loop with list comprehension".to_string())
            }
            ComprehensionType::Extend => Some("Replace for loop with list.extend".to_string()),
        }
    }
}

/// PERF401
pub(crate) fn manual_list_comprehension(checker: &mut Checker, for_stmt: &ast::StmtFor) {
    let Expr::Name(ast::ExprName { id, .. }) = &*for_stmt.target else {
        return;
    };

    let (stmt, if_test) = match &*for_stmt.body {
        // ```python
        // for x in y:
        //     if z:
        //         filtered.append(x)
        // ```
        [Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            test,
            ..
        })] => {
            if !elif_else_clauses.is_empty() {
                return;
            }
            let [stmt] = body.as_slice() else {
                return;
            };
            (stmt, Some(test))
        }
        // ```python
        // for x in y:
        //     filtered.append(f(x))
        // ```
        [stmt] => (stmt, None),
        _ => return,
    };

    let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return;
    };

    let Expr::Call(ast::ExprCall {
        func,
        arguments:
            Arguments {
                args,
                keywords,
                range: _,
            },
        range,
    }) = value.as_ref()
    else {
        return;
    };

    if !keywords.is_empty() {
        return;
    }

    let [arg] = &**args else {
        return;
    };

    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return;
    };

    if attr.as_str() != "append" {
        return;
    }
    // Ignore direct list copies (e.g., `for x in y: filtered.append(x)`), unless it's async, which
    // `manual-list-copy` doesn't cover.
    if !for_stmt.is_async {
        if if_test.is_none() {
            if arg.as_name_expr().is_some_and(|arg| arg.id == *id) {
                return;
            }
        }
    }

    // Avoid, e.g., `for x in y: filtered[x].append(x * x)`.
    if any_over_expr(value, &|expr| {
        expr.as_name_expr().is_some_and(|expr| expr.id == *id)
    }) {
        return;
    }

    // Avoid, e.g., `for x in y: filtered.append(filtered[-1] * 2)`.
    if any_over_expr(arg, &|expr| {
        ComparableExpr::from(expr) == ComparableExpr::from(value)
    }) {
        return;
    }

    // Avoid non-list values.
    let Some(name) = value.as_name_expr() else {
        return;
    };
    let Some(binding) = checker
        .semantic()
        .only_binding(name)
        .map(|id| checker.semantic().binding(id))
    else {
        return;
    };
    if !is_list(binding, checker.semantic()) {
        return;
    }

    // Avoid if the value is used in the conditional test, e.g.,
    //
    // ```python
    // for x in y:
    //    if x in filtered:
    //        filtered.append(x)
    // ```
    //
    // Converting this to a list comprehension would raise a `NameError` as
    // `filtered` is not defined yet:
    //
    // ```python
    // filtered = [x for x in y if x in filtered]
    // ```
    if if_test.is_some_and(|test| {
        any_over_expr(test, &|expr| {
            expr.as_name_expr().is_some_and(|expr| expr.id == name.id)
        })
    }) {
        return;
    }

    let binding_stmt = binding
        .statement(checker.semantic())
        .and_then(|stmt| stmt.as_assign_stmt());

    // If the variable is an empty list literal, then we might be able to replace it with a full list comprehension
    // otherwise, it has to be replaced with a `list.extend`
    let binding_is_empty_list =
        binding_stmt.is_some_and(|binding_stmt| match binding_stmt.value.as_list_expr() {
            Some(list_expr) => list_expr.elts.is_empty(),
            None => false,
        });

    // If the for loop does not have the same parent element as the binding, then it cannot always be
    // deleted and replaced with a list comprehension. This does not apply when using an extend.
    let assignment_in_same_statement = {
        binding.source.is_some_and(|binding_source| {
            let for_loop_parent = checker.semantic().current_statement_parent_id();
            let binding_parent = checker.semantic().parent_statement_id(binding_source);
            for_loop_parent == binding_parent
        })
    };

    // If the binding is not a single name expression, it could be replaced with a list comprehension,
    // but not necessarily, so this needs to be manually fixed. This does not apply when using an extend.
    let binding_has_one_target = {
        match binding_stmt.map(|binding_stmt| binding_stmt.targets.as_slice()) {
            Some([only_target]) => only_target.is_name_expr(),
            _ => false,
        }
    };

    // A list extend works in every context, while a list comprehension only works when all the criteria are true
    let comprehension_type =
        if binding_is_empty_list && assignment_in_same_statement && binding_has_one_target {
            ComprehensionType::ListComprehension
        } else {
            ComprehensionType::Extend
        };

    let mut diagnostic = Diagnostic::new(
        ManualListComprehension {
            is_async: for_stmt.is_async,
            comprehension_type: Some(comprehension_type),
        },
        *range,
    );

    // TODO: once this fix is stabilized, change the rule to always fixable
    if checker.settings.preview.is_enabled() {
        diagnostic.try_set_fix(|| {
            convert_to_list_extend(
                comprehension_type,
                binding,
                for_stmt,
                if_test.map(std::convert::AsRef::as_ref),
                arg,
                checker,
            )
        });
    }

    checker.diagnostics.push(diagnostic);
}

fn convert_to_list_extend(
    fix_type: ComprehensionType,
    binding: &Binding,
    for_stmt: &ast::StmtFor,
    if_test: Option<&ast::Expr>,
    to_append: &Expr,
    checker: &Checker,
) -> Result<Fix> {
    let locator = checker.locator();
    let if_str = match if_test {
        Some(test) => format!(" if {}", locator.slice(test.range())),
        None => String::new(),
    };

    let for_iter_str = locator.slice(for_stmt.iter.range());
    let for_type = if for_stmt.is_async {
        "async for"
    } else {
        "for"
    };
    let target_str = locator.slice(for_stmt.target.range());
    let elt_str = locator.slice(to_append);
    let generator_str = format!("{elt_str} {for_type} {target_str} in {for_iter_str}{if_str}");

    let comment_strings_in_range = |range| {
        checker
            .comment_ranges()
            .comments_in_range(range)
            .iter()
            .map(|range| locator.slice(range).trim_whitespace_start())
            .collect()
    };
    let for_stmt_end = for_stmt.range.end();
    let for_loop_inline_comments: Vec<&str> = comment_strings_in_range(for_stmt.range);
    let for_loop_trailing_comment =
        comment_strings_in_range(TextRange::new(for_stmt_end, locator.line_end(for_stmt_end)));
    let newline = checker.stylist().line_ending().as_str();

    match fix_type {
        ComprehensionType::Extend => {
            let variable_name = checker.locator().slice(binding.range);

            let comprehension_body = format!("{variable_name}.extend({generator_str})");

            let indent_range = TextRange::new(
                locator.line_start(for_stmt.range.start()),
                for_stmt.range.start(),
            );
            let indentation = if for_loop_inline_comments.is_empty() {
                String::new()
            } else {
                format!("{newline}{}", locator.slice(indent_range))
            };
            let text_to_replace = format!(
                "{}{indentation}{comprehension_body}",
                for_loop_inline_comments.join(&indentation)
            );
            Ok(Fix::unsafe_edit(Edit::range_replacement(
                text_to_replace,
                for_stmt.range,
            )))
        }
        ComprehensionType::ListComprehension => {
            let binding_stmt = binding
                .statement(checker.semantic())
                .and_then(|stmt| stmt.as_assign_stmt())
                .ok_or(anyhow!(
                    "Binding must have a statement to convert into a list comprehension"
                ))?;
            let empty_list_to_replace = binding_stmt.value.as_list_expr().ok_or(anyhow!(
                "Assignment value must be an empty list literal in order to replace with a list comprehension"
            ))?;

            let comprehension_body = format!("[{generator_str}]");

            let indent_range = TextRange::new(
                locator.line_start(binding_stmt.range.start()),
                binding_stmt.range.start(),
            );

            let mut for_loop_comments = for_loop_inline_comments;
            for_loop_comments.extend(for_loop_trailing_comment);

            let indentation = if for_loop_comments.is_empty() {
                String::new()
            } else {
                format!("{newline}{}", locator.slice(indent_range))
            };
            let leading_comments = format!("{}{indentation}", for_loop_comments.join(&indentation));

            let mut additional_fixes = vec![Edit::range_deletion(
                locator.full_lines_range(for_stmt.range),
            )];
            // if comments are empty, trying to insert them panics
            if !leading_comments.is_empty() {
                additional_fixes.push(Edit::insertion(
                    leading_comments,
                    binding_stmt.range.start(),
                ));
            }
            Ok(Fix::unsafe_edits(
                Edit::range_replacement(comprehension_body, empty_list_to_replace.range),
                additional_fixes,
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComprehensionType {
    Extend,
    ListComprehension,
}
