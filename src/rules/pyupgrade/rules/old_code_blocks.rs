use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;
use rustpython_parser::ast::{Expr, ExprKind, Stmt, Unaryop, Constant};
use num_bigint::Sign;
use crate::settings::types::PythonVersion;

/// Checks whether the give attribute is from the given path
fn check_path(checker: &Checker, expr: &Expr, path: &[&str]) -> bool {
    checker
        .resolve_call_path(expr)
        .map_or(false, |call_path| call_path.as_slice() == path)
}

/// Returns true if the user's linting version is greater than the version specified in the tuple
fn compare_version(elts: &[Expr], py_version: PythonVersion) -> bool {
    let mut version: Vec<u32> = vec![];
    for elt in elts {
        if let ExprKind::Constant { value: Constant::Int(item), .. } = &elt.node {
            let the_number = item.to_u32_digits();
            match the_number.0 {
                // We do not have a way of handling these values
                Sign::Minus | Sign::NoSign => {
                    return false;
                },
                Sign::Plus => {
                    // Assuming that the version will never be above a 32 bit
                    version.push(*the_number.1.get(0).unwrap())
                }
            }
        } else {
            return false;
        }
    }
    let mut ver_iter = version.iter();
    // Check the first number (the major version)
    if let Some(first) = ver_iter.next() {
        if *first < 3 {
            return true;
        } else if *first == 3 {
            // Check the second number (the minor version)
            if let Some(first) = ver_iter.next() {
                if *first < py_version.to_tuple().1 {
                    return true;
                }
            }
        }
    }
    false
}

/// Converts an if statement that has the py2 block on top
fn fix_py2_block() {
}

/// UP037
pub fn old_code_blocks(checker: &Checker, test: &Expr, body: &[Stmt], orelse: &[Stmt]) {
    // NOTE: Pyupgrade ONLY works if `sys.version_info` is on the left
    match &test.node {
        ExprKind::Compare {
            left,
            ops,
            comparators,
        } => {
            if check_path(checker, left, &["sys", "version_info"]) {
                println!("WE HAVE version_info");
                // We need to ensure we have only one operation and one comparison
                if ops.len() == 1 && comparators.len() == 1 {
                    if let ExprKind::Tuple { elts, ctx } = &comparators.get(0).unwrap().node {
                        println!("{:?}", elts);
                        compare_version(elts, checker.settings.target_version);
                    }
                }
            }
        }
        ExprKind::Attribute { value, attr, ctx } => {
            // if six.PY2
            if check_path(checker, test, &["six", "PY2"]) {}
        }
        ExprKind::UnaryOp { op, operand } => {
            // if not six.PY3
            if check_path(checker, test, &["six", "PY3"]) && op == &Unaryop::Not {}
        }
        _ => (),
    }
}
