use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pyflakes::format::FormatSummary;
use crate::violations;
use rustpython_ast::{Expr, ExprKind};
use std::collections::HashMap;

#[derive(Debug)]
struct FormatFunction {
    args: Vec<String>,
    kwargs: HashMap<String, String>,
}

impl FormatFunction {
    fn new(expr: &Expr) -> Result<Self, ()> {
        println!("expr: {:?}", expr);
        if let ExprKind::Call{ func, args, keywords } = &expr.node {
            println!("{:?}", args);
            println!("{:?}", keywords);
        }
        Ok(Self {
            args: vec![],
            kwargs: HashMap::new(),
        })
    }

    /// Returns true if args and kwargs are empty
    fn is_empty(&self) -> bool {
        self.args.is_empty() && self.kwargs.is_empty()
    }

    fn add_arg(&mut self, arg: String) {
        self.args.push(arg);
    }

    fn add_kwarg(&mut self, key: String, value: String) {
        self.kwargs.insert(key, value);
    }

    /// Returns true if the statement and function call match, and false if not
    fn check_with_summary(&self, summary: &FormatSummary) -> bool {
        summary.autos.len() == self.args.len() && summary.keywords.len() == self.kwargs.len()
    }
}

fn generate_f_string(summary: &FormatSummary, expr: &Expr) -> String {
    let mut original_call = FormatFunction::new(expr);
    println!("{:?}", original_call);
    String::new()
}

/// UP032
pub(crate) fn f_strings(checker: &mut Checker, summary: &FormatSummary, expr: &Expr) {
    if summary.has_nested_parts {
        return;
    }
    // UP030 already removes the indexes, so we should not need to worry about the complexity
    if !summary.indexes.is_empty() {
        return;
    }
    println!("Checkpoint Charlie");
    let mut diagnostic = Diagnostic::new(violations::FString, Range::from_located(expr));
    println!("{:?}", diagnostic.kind.code());
    println!("{:?}", checker.patch(diagnostic.kind.code()));
    // Currently, the only issue we know of is in LibCST:
    // https://github.com/Instagram/LibCST/issues/846
    let contents = generate_f_string(summary, expr);
    if checker.patch(diagnostic.kind.code()) {
        diagnostic.amend(Fix::replacement(
            contents,
            expr.location,
            expr.end_location.unwrap(),
        ));
    };
    checker.diagnostics.push(diagnostic);
}
