use ruff_python_ast::{
    Expr,
    visitor::{self, Visitor},
};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary direct calls to lambda expressions.
///
/// ## Why is this bad?
/// Calling a lambda expression directly is unnecessary. The expression can be
/// executed inline instead to improve readability.
///
/// ## Example
/// ```python
/// area = (lambda r: 3.14 * r**2)(radius)
/// ```
///
/// Use instead:
/// ```python
/// area = 3.14 * radius**2
/// ```
///
/// ## References
/// - [Python documentation: Lambdas](https://docs.python.org/3/reference/expressions.html#lambda)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryDirectLambdaCall;

impl Violation for UnnecessaryDirectLambdaCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Lambda expression called directly. Execute the expression inline instead.".to_string()
    }
}

/// Check if the lambda body contains comprehensions
fn has_comprehensions(expr: &Expr) -> bool {
    let mut finder = ComprehensionFinder { found: false };
    finder.visit_expr(expr);
    finder.found
}

/// Check if a name is used within comprehensions in the expression
fn name_used_in_comprehension(expr: &Expr, target_name: &str) -> bool {
    let mut checker = ComprehensionNameChecker {
        target_name,
        found: false,
        in_comprehension: false,
    };
    checker.visit_expr(expr);
    checker.found
}

/// Check if inlining the lambda would cause undefined name issues in comprehensions within class scopes
fn would_cause_undefined_name_in_comprehension(
    lambda_body: &Expr,
    lambda_params: &[String],
    call_args: &[&Expr],
    checker: &Checker,
) -> bool {
    // Check if we're in a class scope
    if !checker.semantic().current_scope().kind.is_class() {
        return false;
    }

    // Check if the lambda body contains comprehensions
    if !has_comprehensions(lambda_body) {
        return false;
    }

    // Check each lambda parameter to see if inlining would cause issues
    for (param_idx, param_name) in lambda_params.iter().enumerate() {
        // Check if this parameter is used in a comprehension
        if name_used_in_comprehension(lambda_body, param_name) {
            // Get the corresponding call argument
            if let Some(call_arg) = call_args.get(param_idx) {
                // Check if the call argument would cause issues after inlining
                match call_arg {
                    // Simple name that refers to a class variable
                    Expr::Name(arg_name) => {
                        if checker
                            .semantic()
                            .current_scope()
                            .get(arg_name.id.as_str())
                            .is_some()
                        {
                            return true;
                        }
                    }
                    // Attribute access on class variables (like A.threshold)
                    Expr::Attribute(attr_expr) => {
                        if let Expr::Name(base_name) = attr_expr.value.as_ref() {
                            if checker
                                .semantic()
                                .current_scope()
                                .get(base_name.id.as_str())
                                .is_some()
                            {
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    false
}

/// Extracts lambda parameters and call arguments from a lambda call expression.
///
/// This function validates that the expression is a lambda call and extracts the necessary
/// components for further analysis. It returns the lambda parameter names and the call
/// arguments, which can be used to check for potential issues with lambda inlining.
fn extract_lambda_call_components<'a>(
    expr: &'a ruff_python_ast::Expr,
    lambda_expr: &ruff_python_ast::ExprLambda,
) -> Option<(Vec<String>, Vec<&'a Expr>)> {
    if let Expr::Call(call_expr) = expr {
        // Get the lambda parameter names
        let lambda_params: Vec<String> = lambda_expr
            .parameters
            .as_ref()
            .map(|params| {
                params
                    .args
                    .iter()
                    .map(|param| param.parameter.name.to_string())
                    .collect()
            })
            .unwrap_or_default();

        // Get the call arguments
        let call_args: Vec<&Expr> = call_expr.arguments.args.iter().collect();

        Some((lambda_params, call_args))
    } else {
        None
    }
}

/// Visitor to find comprehensions in an expression
struct ComprehensionFinder {
    found: bool,
}

impl Visitor<'_> for ComprehensionFinder {
    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_) => {
                self.found = true;
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

/// Visitor to check if a specific name is referenced within a comprehension
struct ComprehensionNameChecker<'a> {
    target_name: &'a str,
    found: bool,
    in_comprehension: bool,
}

impl Visitor<'_> for ComprehensionNameChecker<'_> {
    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_) => {
                let was_in_comprehension = self.in_comprehension;
                self.in_comprehension = true;
                visitor::walk_expr(self, expr);
                self.in_comprehension = was_in_comprehension;
            }
            Expr::Name(name_expr) if self.in_comprehension => {
                if name_expr.id.as_str() == self.target_name {
                    self.found = true;
                }
                visitor::walk_expr(self, expr);
            }
            _ => {
                visitor::walk_expr(self, expr);
            }
        }
    }
}

/// PLC3002
pub(crate) fn unnecessary_direct_lambda_call(checker: &Checker, expr: &Expr, func: &Expr) {
    if let Expr::Lambda(lambda_expr) = func {
        // Extract lambda parameters and call arguments
        if let Some((lambda_params, call_args)) = extract_lambda_call_components(expr, lambda_expr)
        {
            // Check if inlining would cause undefined name issues in comprehensions
            if would_cause_undefined_name_in_comprehension(
                &lambda_expr.body,
                &lambda_params,
                &call_args,
                checker,
            ) {
                return;
            }
        }

        checker.report_diagnostic(UnnecessaryDirectLambdaCall, expr.range());
    }
}
