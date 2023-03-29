use std::str::FromStr;

use rustc_hash::FxHashMap;
use rustpython_common::cformat::{CFormatPart, CFormatSpec, CFormatStrOrBytes, CFormatString};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Location, Operator};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for mismatched argument types in "old-style" format strings.
///
/// ## Why is this bad?
/// The format string is not checked at compile time, so it is easy to
/// introduce bugs by mistyping the format string.
///
/// ## Example
/// ```python
/// print("%d" % "1")
/// ```
///
/// Use instead:
/// ```python
/// print("%d" % 1)
/// ```
#[violation]
pub struct BadStringFormatType;

impl Violation for BadStringFormatType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Format type does not match argument type")
    }
}

#[derive(Debug, Copy, Clone)]
enum DataType {
    String,
    Integer,
    Float,
    Object,
    Unknown,
}

impl From<&Expr> for DataType {
    fn from(expr: &Expr) -> Self {
        match &expr.node {
            ExprKind::NamedExpr { value, .. } => (&**value).into(),
            ExprKind::UnaryOp { operand, .. } => (&**operand).into(),
            ExprKind::Dict { .. } => DataType::Object,
            ExprKind::Set { .. } => DataType::Object,
            ExprKind::ListComp { .. } => DataType::Object,
            ExprKind::SetComp { .. } => DataType::Object,
            ExprKind::DictComp { .. } => DataType::Object,
            ExprKind::GeneratorExp { .. } => DataType::Object,
            ExprKind::JoinedStr { .. } => DataType::String,
            ExprKind::BinOp { left, op, .. } => {
                // Ex) "a" % "b"
                if matches!(
                    left.node,
                    ExprKind::Constant {
                        value: Constant::Str(..),
                        ..
                    }
                ) && matches!(op, Operator::Mod)
                {
                    return DataType::String;
                }
                DataType::Unknown
            }
            ExprKind::Constant { value, .. } => match value {
                Constant::Str(_) => DataType::String,
                Constant::Int(_) => DataType::Integer,
                Constant::Float(_) => DataType::Float,
                _ => DataType::Unknown,
            },
            ExprKind::List { .. } => DataType::Object,
            ExprKind::Tuple { .. } => DataType::Object,
            _ => DataType::Unknown,
        }
    }
}

impl DataType {
    fn is_compatible_with(self, format: FormatType) -> bool {
        match self {
            DataType::String => matches!(
                format,
                FormatType::Unknown | FormatType::String | FormatType::Repr
            ),
            DataType::Object => matches!(
                format,
                FormatType::Unknown | FormatType::String | FormatType::Repr
            ),
            DataType::Integer => matches!(
                format,
                FormatType::Unknown
                    | FormatType::String
                    | FormatType::Repr
                    | FormatType::Integer
                    | FormatType::Float
                    | FormatType::Number
            ),
            DataType::Float => matches!(
                format,
                FormatType::Unknown
                    | FormatType::String
                    | FormatType::Repr
                    | FormatType::Float
                    | FormatType::Number
            ),
            DataType::Unknown => true,
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum FormatType {
    Repr,
    String,
    Integer,
    Float,
    Number,
    Unknown,
}

impl From<char> for FormatType {
    fn from(format: char) -> Self {
        match format {
            'r' => FormatType::Repr,
            's' => FormatType::String,
            // The python documentation says "d" only works for integers, but it works for floats as
            // well: https://docs.python.org/3/library/string.html#formatstrings
            // I checked the rest of the integer codes, and none of them work with floats
            'n' | 'd' => FormatType::Number,
            'b' | 'c' | 'o' | 'x' | 'X' => FormatType::Integer,
            'e' | 'E' | 'f' | 'F' | 'g' | 'G' | '%' => FormatType::Float,
            _ => FormatType::Unknown,
        }
    }
}

fn collect_specs(formats: &[CFormatStrOrBytes<String>]) -> Vec<&CFormatSpec> {
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

/// Return `true` if the format string is equivalent to the constant type
fn equivalent(format: &CFormatSpec, value: &Expr) -> bool {
    let constant: DataType = value.into();
    let format: FormatType = format.format_char.into();
    constant.is_compatible_with(format)
}

/// Return `true` if the [`Constnat`] aligns with the format type.
fn is_valid_constant(formats: &[CFormatStrOrBytes<String>], value: &Expr) -> bool {
    let formats = collect_specs(formats);
    // If there is more than one format, this is not valid python and we should
    // return true so that no error is reported
    if formats.len() != 1 {
        return true;
    }
    let format = formats[0];
    equivalent(format, value)
}

/// Return `true` if the tuple elements align with the format types.
fn is_valid_tuple(formats: &[CFormatStrOrBytes<String>], elts: &[Expr]) -> bool {
    let formats = collect_specs(formats);

    // If there are more formats that values, the statement is invalid. Avoid
    // checking the values.
    if formats.len() > elts.len() {
        return true;
    }

    for (format, elt) in formats.iter().zip(elts) {
        if !equivalent(format, elt) {
            return false;
        }
    }
    true
}

/// Return `true` if the dictionary values align with the format types.
fn is_valid_dict(
    formats: &[CFormatStrOrBytes<String>],
    keys: &[Option<Expr>],
    values: &[Expr],
) -> bool {
    let formats = collect_specs(formats);

    // If there are more formats that values, the statement is invalid. Avoid
    // checking the values.
    if formats.len() > values.len() {
        return true;
    }

    let formats_hash: FxHashMap<&str, &&CFormatSpec> = formats
        .iter()
        .filter_map(|format| {
            format
                .mapping_key
                .as_ref()
                .map(|mapping_key| (mapping_key.as_str(), format))
        })
        .collect();
    for (key, value) in keys.iter().zip(values) {
        let Some(key) = key else {
            return true;
        };
        if let ExprKind::Constant {
            value: Constant::Str(mapping_key),
            ..
        } = &key.node
        {
            let Some(format) = formats_hash.get(mapping_key.as_str()) else {
                return true;
            };
            if !equivalent(format, value) {
                return false;
            }
        } else {
            // We can't check non-string keys.
            return true;
        }
    }
    true
}

/// PLE1307
pub fn bad_string_format_type(checker: &mut Checker, expr: &Expr, right: &Expr) {
    // Grab each string segment (in case there's an implicit concatenation).
    let content = checker.locator.slice(expr);
    let mut strings: Vec<(Location, Location)> = vec![];
    for (start, tok, end) in lexer::lex_located(content, Mode::Module, expr.location).flatten() {
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
        let string = checker.locator.slice(Range::new(*start, *end));
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
    let is_valid = match &right.node {
        ExprKind::Tuple { elts, .. } => is_valid_tuple(&format_strings, elts),
        ExprKind::Dict { keys, values } => is_valid_dict(&format_strings, keys, values),
        ExprKind::Constant { .. } => is_valid_constant(&format_strings, right),
        _ => true,
    };
    if !is_valid {
        checker
            .diagnostics
            .push(Diagnostic::new(BadStringFormatType, Range::from(expr)));
    }
}
