use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_semantic::{analyze::typing::is_list, Binding};
use ruff_text_size::TextRange;

use anyhow::{anyhow, Result};

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
#[violation]
pub struct ManualListComprehension {
    is_async: bool,
    comprehension_type: Option<ComprehensionType>,
}

impl Violation for ManualListComprehension {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ManualListComprehension { is_async, .. } = self;
        match is_async {
            false => format!("Use a list comprehension to create a transformed list"),
            true => format!("Use an async list comprehension to create a transformed list"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        self.comprehension_type
            .map(|comprehension_type| match comprehension_type {
                ComprehensionType::ListComprehension => {
                    format!("Replace for loop with list comprehension")
                }
                ComprehensionType::Extend => format!("Replace for loop with list.extend"),
            })
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

    let comprehension_type = if binding_is_empty_list {
        ComprehensionType::ListComprehension
    } else {
        ComprehensionType::Extend
    };

    // If the for loop does not have the same parent element as the binding, then it cannot always be
    // deleted and replaced with a list comprehension. This does not apply when using an extend.
    let assignment_in_same_statement = {
        let for_loop_parent = checker.semantic().current_statement_parent_id();

        binding.source.is_some_and(|binding_source| {
            let binding_parent = checker.semantic().parent_statement_id(binding_source);
            for_loop_parent == binding_parent
        })
    };

    // If the binding is not a single name expression, it could be replaced with a list comprehension,
    // but not necessarily, so this needs to be manually fixed. This does not apply when using an extend.
    let binding_has_one_target = {
        let only_target = binding_stmt.is_some_and(|binding_stmt| binding_stmt.targets.len() == 1);
        let is_name =
            binding_stmt.is_some_and(|binding_stmt| binding_stmt.targets[0].is_name_expr());
        only_target && is_name
    };

    // A list extend works in every context, while a list comprehension only works when all the criteria are true
    let comprehension_type =
        Some(comprehension_type).filter(|comprehension_type| match comprehension_type {
            ComprehensionType::ListComprehension => {
                binding_stmt.is_some() && assignment_in_same_statement && binding_has_one_target
            }
            ComprehensionType::Extend => true,
        });

    let mut diagnostic = Diagnostic::new(
        ManualListComprehension {
            is_async: for_stmt.is_async,
            comprehension_type,
        },
        *range,
    );

    diagnostic.try_set_optional_fix(|| match comprehension_type {
        Some(comprehension_type) => convert_to_list_extend(
            comprehension_type,
            binding,
            for_stmt,
            if_test.map(std::convert::AsRef::as_ref),
            arg,
            checker,
        )
        .map(Some),
        None => Ok(None),
    });

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
    let comprehension = ast::Comprehension {
        target: (*for_stmt.target).clone(),
        iter: (*for_stmt.iter).clone(),
        is_async: for_stmt.is_async,
        ifs: if_test.into_iter().cloned().collect(),
        range: TextRange::default(),
    };
    match fix_type {
        ComprehensionType::Extend => {
            let generator = ast::ExprGenerator {
                elt: Box::new(to_append.clone()),
                generators: vec![comprehension],
                parenthesized: false,
                range: TextRange::default(),
            };

            let variable_name = checker.locator().slice(binding.range);

            let extend = ast::ExprAttribute {
                value: Box::new(Expr::Name(ast::ExprName {
                    id: ast::name::Name::new(variable_name),
                    ctx: ast::ExprContext::Load,
                    range: TextRange::default(),
                })),
                attr: ast::Identifier::new("extend", TextRange::default()),
                ctx: ast::ExprContext::Load,
                range: TextRange::default(),
            };

            let list_extend = ast::ExprCall {
                func: Box::new(ast::Expr::Attribute(extend)),
                arguments: Arguments {
                    args: Box::new([ast::Expr::Generator(generator)]),
                    range: TextRange::default(),
                    keywords: Box::new([]),
                },
                range: TextRange::default(),
            };

            let comprehension_body = checker.generator().expr(&Expr::Call(list_extend));

            Ok(Fix::unsafe_edit(Edit::range_replacement(
                comprehension_body,
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

            let list_comp = ast::ExprListComp {
                elt: Box::new(to_append.clone()),
                generators: vec![comprehension],
                range: TextRange::default(),
            };

            let comprehension_body = checker.generator().expr(&Expr::ListComp(list_comp));
            Ok(Fix::unsafe_edits(
                Edit::range_replacement(comprehension_body, empty_list_to_replace.range),
                [Edit::range_deletion(for_stmt.range)],
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComprehensionType {
    Extend,
    ListComprehension,
}
