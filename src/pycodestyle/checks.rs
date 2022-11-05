use itertools::izip;
use rustpython_ast::Location;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind, Unaryop};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind, RejectedCmpop};
use crate::source_code_locator::SourceCodeLocator;

fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

/// E741
pub fn ambiguous_variable_name(name: &str, location: Range) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousVariableName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// E742
pub fn ambiguous_class_name(name: &str, location: Range) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousClassName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// E743
pub fn ambiguous_function_name(name: &str, location: Range) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousFunctionName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// E731
pub fn do_not_assign_lambda(value: &Expr, location: Range) -> Option<Check> {
    if let ExprKind::Lambda { .. } = &value.node {
        Some(Check::new(CheckKind::DoNotAssignLambda, location))
    } else {
        None
    }
}

/// E713, E714
pub fn not_tests(
    op: &Unaryop,
    operand: &Expr,
    check_not_in: bool,
    check_not_is: bool,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    if matches!(op, Unaryop::Not) {
        if let ExprKind::Compare { ops, .. } = &operand.node {
            for op in ops {
                match op {
                    Cmpop::In => {
                        if check_not_in {
                            checks.push(Check::new(
                                CheckKind::NotInTest,
                                Range::from_located(operand),
                            ));
                        }
                    }
                    Cmpop::Is => {
                        if check_not_is {
                            checks.push(Check::new(
                                CheckKind::NotIsTest,
                                Range::from_located(operand),
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    checks
}

/// E711, E712
pub fn literal_comparisons(
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
    check_none_comparisons: bool,
    check_true_false_comparisons: bool,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    let op = ops.first().unwrap();
    let comparator = left;

    // Check `left`.
    if check_none_comparisons
        && matches!(
            comparator.node,
            ExprKind::Constant {
                value: Constant::None,
                kind: None
            }
        )
    {
        if matches!(op, Cmpop::Eq) {
            checks.push(Check::new(
                CheckKind::NoneComparison(RejectedCmpop::Eq),
                Range::from_located(comparator),
            ));
        }
        if matches!(op, Cmpop::NotEq) {
            checks.push(Check::new(
                CheckKind::NoneComparison(RejectedCmpop::NotEq),
                Range::from_located(comparator),
            ));
        }
    }

    if check_true_false_comparisons {
        if let ExprKind::Constant {
            value: Constant::Bool(value),
            kind: None,
        } = comparator.node
        {
            if matches!(op, Cmpop::Eq) {
                checks.push(Check::new(
                    CheckKind::TrueFalseComparison(value, RejectedCmpop::Eq),
                    Range::from_located(comparator),
                ));
            }
            if matches!(op, Cmpop::NotEq) {
                checks.push(Check::new(
                    CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                    Range::from_located(comparator),
                ));
            }
        }
    }

    // Check each comparator in order.
    for (op, comparator) in izip!(ops, comparators) {
        if check_none_comparisons
            && matches!(
                comparator.node,
                ExprKind::Constant {
                    value: Constant::None,
                    kind: None
                }
            )
        {
            if matches!(op, Cmpop::Eq) {
                checks.push(Check::new(
                    CheckKind::NoneComparison(RejectedCmpop::Eq),
                    Range::from_located(comparator),
                ));
            }
            if matches!(op, Cmpop::NotEq) {
                checks.push(Check::new(
                    CheckKind::NoneComparison(RejectedCmpop::NotEq),
                    Range::from_located(comparator),
                ));
            }
        }

        if check_true_false_comparisons {
            if let ExprKind::Constant {
                value: Constant::Bool(value),
                kind: None,
            } = comparator.node
            {
                if matches!(op, Cmpop::Eq) {
                    checks.push(Check::new(
                        CheckKind::TrueFalseComparison(value, RejectedCmpop::Eq),
                        Range::from_located(comparator),
                    ));
                }
                if matches!(op, Cmpop::NotEq) {
                    checks.push(Check::new(
                        CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                        Range::from_located(comparator),
                    ));
                }
            }
        }
    }

    checks
}

/// E721
pub fn type_comparison(ops: &[Cmpop], comparators: &[Expr], location: Range) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    for (op, right) in izip!(ops, comparators) {
        if matches!(op, Cmpop::Is | Cmpop::IsNot | Cmpop::Eq | Cmpop::NotEq) {
            match &right.node {
                ExprKind::Call { func, args, .. } => {
                    if let ExprKind::Name { id, .. } = &func.node {
                        // Ex) type(False)
                        if id == "type" {
                            if let Some(arg) = args.first() {
                                // Allow comparison for types which are not obvious.
                                if !matches!(arg.node, ExprKind::Name { .. }) {
                                    checks.push(Check::new(CheckKind::TypeComparison, location));
                                }
                            }
                        }
                    }
                }
                ExprKind::Attribute { value, .. } => {
                    if let ExprKind::Name { id, .. } = &value.node {
                        // Ex) types.IntType
                        if id == "types" {
                            checks.push(Check::new(CheckKind::TypeComparison, location));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    checks
}

// See: https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
const VALID_ESCAPE_SEQUENCES: &[char; 23] = &[
    '\n', '\\', '\'', '"', 'a', 'b', 'f', 'n', 'r', 't', 'v', '0', '1', '2', '3', '4', '5', '6',
    '7', 'x', // Escape sequences only recognized in string literals
    'N', 'u', 'U',
];

/// Return the quotation markers used for a String token.
fn extract_quote(text: &str) -> &str {
    for quote in ["'''", "\"\"\"", "'", "\""] {
        if text.ends_with(quote) {
            return quote;
        }
    }

    panic!("Unable to find quotation mark for String token.")
}

/// W605
pub fn invalid_escape_sequence(
    locator: &SourceCodeLocator,
    start: &Location,
    end: &Location,
) -> Vec<Check> {
    let mut checks = vec![];

    let text = locator.slice_source_code_range(&Range {
        location: *start,
        end_location: *end,
    });

    // Determine whether the string is single- or triple-quoted.
    let quote = extract_quote(&text);
    let quote_pos = text.find(quote).unwrap();
    let prefix = text[..quote_pos].to_lowercase();
    let body = &text[(quote_pos + quote.len())..(text.len() - quote.len())];

    if !prefix.contains('r') {
        for (row_offset, line) in body.lines().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            for col_offset in 0..chars.len() {
                if chars[col_offset] == '\\' {
                    // If the previous character was also a backslash, skip.
                    if col_offset > 0 && chars[col_offset - 1] == '\\' {
                        continue;
                    }

                    // If we're at the end of the line, skip.
                    if col_offset == chars.len() - 1 {
                        continue;
                    }

                    // If the next character is a valid escape sequence, skip.
                    let next_char = chars[col_offset + 1];
                    if VALID_ESCAPE_SEQUENCES.contains(&next_char) {
                        continue;
                    }

                    // Compute the location of the escape sequence by offsetting the location of the
                    // string token by the characters we've seen thus far.
                    let location = if row_offset == 0 {
                        Location::new(
                            start.row() + row_offset,
                            start.column() + prefix.len() + quote.len() + col_offset,
                        )
                    } else {
                        Location::new(start.row() + row_offset, col_offset)
                    };
                    let end_location = Location::new(location.row(), location.column() + 2);
                    checks.push(Check::new(
                        CheckKind::InvalidEscapeSequence(next_char),
                        Range {
                            location,
                            end_location,
                        },
                    ))
                }
            }
        }
    }

    checks
}
