use crate::define_violation;
use ruff_macros::derive_message_formats;

use std::str::FromStr;

use rustpython_ast::Location;
use rustpython_common::cformat::{CFormatPart, CFormatSpec, CFormatStrOrBytes, CFormatString};
use rustpython_parser::ast::{Constant, Expr, ExprKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::{leading_quote, trailing_quote};
use crate::violation::Violation;
use std::collections::HashMap;

define_violation!(
    pub struct BadStringFormatType;
);
impl Violation for BadStringFormatType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Format type does not match argument type")
    }
}

enum DataType {
    String,
    Integer,
    Float,
    // Number can be float or integer
    Number,
    Other,
}

impl PartialEq for DataType {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (DataType::String, DataType::String)
                | (DataType::Integer, DataType::Integer | DataType::Number)
                | (DataType::Float, DataType::Float | DataType::Number)
                | (
                    DataType::Number,
                    DataType::Number | DataType::Integer | DataType::Float
                )
        )
    }
}

fn char_to_data(format: char) -> DataType {
    match format {
        's' => DataType::String,
        'n' => DataType::Number,
        'b' | 'c' | 'd' | 'o' | 'x' | 'X' => DataType::Integer,
        'e' | 'E' | 'f' | 'F' | 'g' | 'G' | '%' => DataType::Float,
        _ => DataType::Other,
    }
}

fn constant_to_data(value: &Constant) -> DataType {
    match value {
        Constant::Str(_) => DataType::String,
        Constant::Int(_) => DataType::Integer,
        Constant::Float(_) => DataType::Float,
        _ => DataType::Other,
    }
}

fn get_all_specs(formats: &[CFormatStrOrBytes<String>]) -> Vec<&CFormatSpec> {
    let mut specs = vec![];
    for format in formats {
        for (_, item) in format.iter() {
            if let CFormatPart::Spec(spec) = item {
                specs.push(spec);
            }
        }
    }
    specs
}

/// Returns true if the format string is not equivalent to the constant type
fn equivalent(format: &CFormatSpec, value: &Constant) -> bool {
    let clean_constant = constant_to_data(value);
    let clean_format = char_to_data(format.format_char);
    match clean_format {
        // We can ALWAYS format as string
        DataType::String => true,
        _ => {
            if let DataType::Other = clean_constant {
                // If the format is not string, we cannot format other
                false
            } else {
                clean_constant == clean_format
            }
        }
    }
}

/// Checks if the format string matches the constant type formatting it
fn check_constant(formats: &[CFormatStrOrBytes<String>], value: &Constant) -> bool {
    let formats = get_all_specs(formats);
    // If there is more than one format, this is not valid python and we should return true so that
    // no error is reported
    if formats.len() != 1 {
        return true;
    }
    let format = formats.get(0).unwrap();
    equivalent(format, value)
}

fn check_tuple(formats: &[CFormatStrOrBytes<String>], elts: &[Expr]) -> bool {
    let formats = get_all_specs(formats);
    // If there are more formats that values, the statement is invalid
    if formats.len() > elts.len() {
        return true;
    }

    for (format, elt) in formats.iter().zip(elts) {
        if let ExprKind::Constant { value, .. } = &elt.node {
            if !equivalent(format, value) {
                return false;
            }
        // Names cannot be understood yet
        } else if let ExprKind::Name { .. } = &elt.node {
            continue;
        // Non-Constant values can only be formatted as string
        } else if format.format_char != 's' {
            return false;
        }
    }
    true
}

fn check_dict(
    formats: &[CFormatStrOrBytes<String>],
    keys: &[Option<Expr>],
    values: &[Expr],
) -> bool {
    let formats = get_all_specs(formats);
    // If there are more formats that values, the statement is invalid
    if formats.len() > values.len() {
        return true;
    }
    let formats_hash: HashMap<String, &&CFormatSpec> = formats
        .iter()
        .map(|format| (format.mapping_key.clone().unwrap(), format))
        .collect();
    for (key, value) in keys.iter().zip(values) {
        let clean_key = match key {
            Some(key) => key,
            None => return true,
        };
        if let ExprKind::Constant {
            value: Constant::Str(item),
            ..
        } = &clean_key.node
        {
            let format = formats_hash.get(item).unwrap();
            if let ExprKind::Constant { value, .. } = &value.node {
                if !equivalent(format, value) {
                    return false;
                }
            // Non-Constant values can only be formatted as string
            } else if let ExprKind::Name { .. } = &value.node {
                continue;
            // Non-Constant values can only be formatted as string
            } else if format.format_char != 's' {
                return false;
            }
        } else {
            // If the key is not a string, we cannot check it
            return true;
        }
    }
    true
}

fn check_other(formats: &[CFormatStrOrBytes<String>]) -> bool {
    let formats = get_all_specs(formats);
    // If there is more than one format the code is not valid, do not check this error
    if formats.len() != 1 {
        return true;
    }
    formats.get(0).unwrap().format_char == 's'
}

/// PLE1307
pub fn bad_string_format_type(checker: &mut Checker, expr: &Expr, left: &Expr, right: &Expr) {
    // If the modulo symbol is on a separate line, abort.
    if right.location.row() != left.end_location.unwrap().row() {
        return;
    }

    // Grab each string segment (in case there's an implicit concatenation).
    let mut strings: Vec<(Location, Location)> = vec![];
    for (start, tok, end) in lexer::make_tokenizer_located(
        checker
            .locator
            .slice_source_code_range(&Range::from_located(expr)),
        expr.location,
    )
    .flatten()
    {
        if matches!(tok, Tok::String { .. }) {
            strings.push((start, end));
        } else if matches!(tok, Tok::Percent) {
            // Break as soon as we find the modulo symbol.
            break;
        }
    }

    // If there are no string segments, abort.
    if strings.is_empty() {
        return;
    }

    // Parse each string segment.
    let mut format_strings = vec![];
    for (start, end) in &strings {
        let string = checker
            .locator
            .slice_source_code_range(&Range::new(*start, *end));
        let (Some(leader), Some(trailer)) = (leading_quote(string), trailing_quote(string)) else {
            return;
        };
        let string = &string[leader.len()..string.len() - trailer.len()];

        // Parse the format string (e.g. `"%s"`) into a list of `PercentFormat`.
        if let Ok(format_string) = CFormatString::from_str(string) {
            format_strings.push(format_string);
        };
    }

    // Parse the parameters.
    let valid = match &right.node {
        ExprKind::Tuple { elts, .. } => check_tuple(&format_strings, elts),
        ExprKind::Dict { keys, values } => check_dict(&format_strings, keys, values),
        ExprKind::Constant { value, .. } => check_constant(&format_strings, value),
        // TODO: find a way to understand variables
        ExprKind::Name { .. } => true,
        _ => check_other(&format_strings),
    };
    if !valid {
        checker.diagnostics.push(Diagnostic::new(
            BadStringFormatType,
            Range::from_located(expr),
        ));
    }
}
