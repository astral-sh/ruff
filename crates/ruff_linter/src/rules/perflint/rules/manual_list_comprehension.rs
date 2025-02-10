use ruff_python_ast::{self as ast, Arguments, Expr};

use crate::checkers::ast::Checker;
use anyhow::{anyhow, Result};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};

use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_semantic::{analyze::typing::is_list, Binding};
use ruff_python_trivia::{BackwardsTokenizer, PythonWhitespace, SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

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
///
/// Take care that if the original for-loop uses an assignment expression
/// as a conditional, such as `if match:=re.match("\d+","123")`, then
/// the corresponding comprehension must wrap the assignment
/// expression in parentheses to avoid a syntax error.
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
pub(crate) fn manual_list_comprehension(checker: &Checker, for_stmt: &ast::StmtFor) {
    let Expr::Name(ast::ExprName {
        id: for_stmt_target_id,
        ..
    }) = &*for_stmt.target
    else {
        return;
    };

    let (stmt, if_test) = match &*for_stmt.body {
        // ```python
        // for x in y:
        //     if z:
        //         filtered.append(x)
        // ```
        [ast::Stmt::If(ast::StmtIf {
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

    let ast::Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
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

    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = &**func else {
        return;
    };

    if attr.as_str() != "append" {
        return;
    }

    // Avoid non-list values.
    let Some(list_name) = value.as_name_expr() else {
        return;
    };

    // Ignore direct list copies (e.g., `for x in y: filtered.append(x)`), unless it's async, which
    // `manual-list-copy` doesn't cover.
    if !for_stmt.is_async {
        if if_test.is_none() {
            if arg
                .as_name_expr()
                .is_some_and(|arg| arg.id == *for_stmt_target_id)
            {
                return;
            }
        }
    }

    // Avoid, e.g., `for x in y: filtered.append(filtered[-1] * 2)`.
    if any_over_expr(arg, &|expr| {
        expr.as_name_expr()
            .is_some_and(|expr| expr.id == list_name.id)
    }) {
        return;
    }

    let Some(list_binding) = checker
        .semantic()
        .only_binding(list_name)
        .map(|id| checker.semantic().binding(id))
    else {
        return;
    };

    if !is_list(list_binding, checker.semantic()) {
        return;
    }

    // Avoid if the list is used in the conditional test, e.g.,
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
            expr.as_name_expr()
                .is_some_and(|expr| expr.id == list_name.id)
        })
    }) {
        return;
    }

    // Avoid if the for-loop target is used outside the for loop, e.g.,
    //
    // ```python
    // for x in y:
    //     filtered.append(x)
    // print(x)
    // ```
    //
    // If this were a comprehension, x would no longer have the correct scope:
    //
    // ```python
    // filtered = [x for x in y]
    // print(x)
    // ```
    let target_binding = checker
        .semantic()
        .bindings
        .iter()
        .find(|binding| for_stmt.target.range() == binding.range)
        .unwrap();
    // If any references to the loop target variable are after the loop,
    // then converting it into a comprehension would cause a NameError
    if target_binding
        .references()
        .map(|reference| checker.semantic().reference(reference))
        .any(|other_reference| for_stmt.end() < other_reference.start())
    {
        return;
    }

    let list_binding_stmt = list_binding.statement(checker.semantic());
    let list_binding_value = list_binding_stmt.and_then(|binding_stmt| match binding_stmt {
        ast::Stmt::AnnAssign(assign) => assign.value.as_deref(),
        ast::Stmt::Assign(assign) => Some(&assign.value),
        _ => None,
    });
    // If the variable is an empty list literal, then we might be able to replace it with a full list comprehension
    // otherwise, it has to be replaced with a `list.extend`
    let binding_is_empty_list =
        list_binding_value.is_some_and(|binding_value| match binding_value.as_list_expr() {
            Some(list_expr) => list_expr.elts.is_empty(),
            None => false,
        });

    // If the for loop does not have the same parent element as the binding, then it cannot always be
    // deleted and replaced with a list comprehension. This does not apply when using `extend`.
    let assignment_in_same_statement = {
        list_binding.source.is_some_and(|binding_source| {
            let for_loop_parent = checker.semantic().current_statement_parent_id();
            let binding_parent = checker.semantic().parent_statement_id(binding_source);
            for_loop_parent == binding_parent
        })
    };

    // If the binding is not a single name expression, it could be replaced with a list comprehension,
    // but not necessarily, so this needs to be manually fixed. This does not apply when using an extend.
    let binding_has_one_target = list_binding_stmt.is_some_and(|binding_stmt| match binding_stmt {
        ast::Stmt::AnnAssign(_) => true,
        ast::Stmt::Assign(assign) => assign.targets.len() == 1,
        _ => false,
    });

    // If the binding gets used in between the assignment and the for loop, a list comprehension is no longer safe

    // If the binding is after the for loop, then it can't be fixed, and this check would panic,
    // so we check that they are in the same statement first
    let binding_unused_between = assignment_in_same_statement
        && list_binding_stmt.is_some_and(|binding_stmt| {
            let from_assign_to_loop = TextRange::new(binding_stmt.end(), for_stmt.start());
            // Test if there's any reference to the list symbol between its definition and the for loop.
            // if there's at least one, then it's been accessed in the middle somewhere, so it's not safe to change into a list comprehension
            !list_binding
                .references()
                .map(|ref_id| checker.semantic().reference(ref_id).range())
                .any(|text_range| from_assign_to_loop.contains_range(text_range))
        });

    // A list extend works in every context, while a list comprehension only works when all the criteria are true
    let comprehension_type = if binding_is_empty_list
        && assignment_in_same_statement
        && binding_has_one_target
        && binding_unused_between
    {
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
                list_binding,
                for_stmt,
                if_test.map(std::convert::AsRef::as_ref),
                arg,
                checker,
            )
        });
    }

    checker.report_diagnostic(diagnostic);
}

fn convert_to_list_extend(
    fix_type: ComprehensionType,
    binding: &Binding,
    for_stmt: &ast::StmtFor,
    if_test: Option<&ast::Expr>,
    to_append: &Expr,
    checker: &Checker,
) -> Result<Fix> {
    let semantic = checker.semantic();
    let locator = checker.locator();
    let if_str = match if_test {
        Some(test) => {
            // If the test is an assignment expression,
            // we must parenthesize it when it appears
            // inside the comprehension to avoid a syntax error.
            //
            // Notice that we do not need `any_over_expr` here,
            // since if the assignment expression appears
            // internally (e.g. as an operand in a boolean
            // operation) then it will already be parenthesized.
            if test.is_named_expr() {
                format!(" if ({})", locator.slice(test.range()))
            } else {
                format!(" if {}", locator.slice(test.range()))
            }
        }
        None => String::new(),
    };

    // if the loop target was an implicit tuple, add parentheses around it
    // ```python
    //  for i in a, b:
    //      ...
    // ```
    // becomes
    // [... for i in (a, b)]
    let for_iter_str = if for_stmt
        .iter
        .as_ref()
        .as_tuple_expr()
        .is_some_and(|expr| !expr.parenthesized)
    {
        format!("({})", locator.slice(for_stmt.iter.range()))
    } else {
        locator.slice(for_stmt.iter.range()).to_string()
    };

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
            // Ignore comments inside of the append or iterator, since these are preserved
            .filter(|comment| {
                !to_append.range().contains_range(**comment)
                    && !for_stmt.iter.range().contains_range(**comment)
            })
            .map(|range| locator.slice(range).trim_whitespace_start())
            .collect()
    };

    let variable_name = locator.slice(binding);
    let for_loop_inline_comments: Vec<&str> = comment_strings_in_range(for_stmt.range);

    let newline = checker.stylist().line_ending().as_str();

    let indent = locator.slice(TextRange::new(
        locator.line_start(for_stmt.range.start()),
        for_stmt.range.start(),
    ));

    match fix_type {
        ComprehensionType::Extend => {
            let generator_str = if for_stmt.is_async {
                // generators do not implement __iter__, so `async for` requires the generator to be a list
                format!("[{generator_str}]")
            } else {
                generator_str
            };

            let comprehension_body = format!("{variable_name}.extend({generator_str})");

            let indentation = if for_loop_inline_comments.is_empty() {
                String::new()
            } else {
                format!("{newline}{indent}")
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
            let binding_stmt = binding.statement(semantic);
            let binding_stmt_range = binding_stmt
                .and_then(|stmt| match stmt {
                    ast::Stmt::AnnAssign(assign) => Some(assign.range),
                    ast::Stmt::Assign(assign) => Some(assign.range),
                    _ => None,
                })
                .ok_or(anyhow!(
                    "Binding must have a statement to convert into a list comprehension"
                ))?;
            // If the binding has multiple statements on its line, the fix would be substantially more complicated
            let (semicolon_before, after_semicolon) = {
                // determine whether there's a semicolon either before or after the binding statement.
                // Since it's a binding statement, we can just check whether there's a semicolon immediately
                // after the whitespace in front of or behind it
                let mut after_tokenizer =
                    SimpleTokenizer::starts_at(binding_stmt_range.end(), locator.contents())
                        .skip_trivia();

                let after_semicolon = if after_tokenizer
                    .next()
                    .is_some_and(|token| token.kind() == SimpleTokenKind::Semi)
                {
                    after_tokenizer.next()
                } else {
                    None
                };

                let semicolon_before = BackwardsTokenizer::up_to(
                    binding_stmt_range.start(),
                    locator.contents(),
                    checker.comment_ranges(),
                )
                .skip_trivia()
                .next()
                .filter(|token| token.kind() == SimpleTokenKind::Semi);

                (semicolon_before, after_semicolon)
            };
            // If there are multiple binding statements in one line, we don't want to accidentally delete them
            // Instead, we just delete the binding statement and leave any comments where they are
            let (binding_stmt_deletion_range, binding_is_multiple_stmts) =
                match (semicolon_before, after_semicolon) {
                    // ```python
                    // a = []
                    // ```
                    (None, None) => (locator.full_lines_range(binding_stmt_range), false),

                    // ```python
                    // a = 1; b = []
                    //      ^^^^^^^^
                    // a = 1; b = []; c = 3
                    //      ^^^^^^^^
                    // ```
                    (Some(semicolon_before), Some(_) | None) => (
                        TextRange::new(semicolon_before.start(), binding_stmt_range.end()),
                        true,
                    ),

                    // ```python
                    // a = []; b = 3
                    // ^^^^^^^
                    // ```
                    (None, Some(after_semicolon)) => (
                        TextRange::new(binding_stmt_range.start(), after_semicolon.start()),
                        true,
                    ),
                };

            let annotations = match binding_stmt.and_then(|stmt| stmt.as_ann_assign_stmt()) {
                Some(assign) => format!(": {}", locator.slice(assign.annotation.range())),
                None => String::new(),
            };

            let mut comments_to_move = for_loop_inline_comments;
            if !binding_is_multiple_stmts {
                comments_to_move.extend(comment_strings_in_range(binding_stmt_deletion_range));
            }

            let indentation = if comments_to_move.is_empty() {
                String::new()
            } else {
                format!("{newline}{indent}")
            };
            let leading_comments = format!("{}{indentation}", comments_to_move.join(&indentation));

            let comprehension_body =
                format!("{leading_comments}{variable_name}{annotations} = [{generator_str}]");

            Ok(Fix::unsafe_edits(
                Edit::range_deletion(binding_stmt_deletion_range),
                [Edit::range_replacement(comprehension_body, for_stmt.range)],
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComprehensionType {
    Extend,
    ListComprehension,
}
