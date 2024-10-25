use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_semantic::analyze::typing::is_list;
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

    let Some(Stmt::Assign(binding_stmt)) = binding.statement(checker.semantic()) else {
        return;
    };

    // If the variable is an empty list literal, then we might be able to replace it with a full list comprehension
    // otherwise, it has to be replaced with a `list.extend`
    let binding_is_empty_list = match binding_stmt.value.as_ref() {
        Expr::List(ast::ExprList { elts, .. }) => elts.is_empty(),
        _ => false,
    };
    let comprehension_type = if binding_is_empty_list {
        ComprehensionType::ListComprehension
    } else {
        ComprehensionType::Extend
    };

    // If the for loop does not have the same parent element as the binding, then it cannot be
    // deleted and replaced with a list comprehension.
    let assignment_in_same_statement = {
        let for_loop_parent = checker.semantic().current_statement_parent_id();
        let Some(binding_source) = binding.source else {
            return;
        };
        let binding_parent = checker.semantic().parent_statement_id(binding_source);
        for_loop_parent == binding_parent
    };

    let comprehension_type = Some(comprehension_type).filter(|_| assignment_in_same_statement);

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
            binding_stmt,
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
    binding_stmt: &ast::StmtAssign,
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

            let [variable_name] = &binding_stmt.targets[..] else {
                return Err(anyhow!(
                    "Binding now has multiple targets when it previously had one"
                ));
            };

            let extend = ast::ExprAttribute {
                value: Box::new(variable_name.clone()),
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
            let Expr::List(assignment_value) = binding_stmt.value.as_ref() else {
                return Err(anyhow!(
                    "Assignment value changed from list literal into another type"
                ));
            };
            let list_comp = ast::ExprListComp {
                elt: Box::new(to_append.clone()),
                generators: vec![comprehension],
                range: TextRange::default(),
            };
            let comprehension_body = checker.generator().expr(&Expr::ListComp(list_comp));
            Ok(Fix::unsafe_edits(
                Edit::range_replacement(comprehension_body, assignment_value.range),
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
