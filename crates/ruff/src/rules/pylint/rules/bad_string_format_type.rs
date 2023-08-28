use std::str::FromStr;

use ruff_python_ast::{self as ast, Constant, Expr};
use ruff_python_literal::cformat::{CFormatPart, CFormatSpec, CFormatStrOrBytes, CFormatString};
use ruff_python_parser::{lexer, AsMode};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};

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
enum FormatType {
    Repr,
    String,
    Integer,
    Float,
    Number,
    Unknown,
}

impl FormatType {
    fn is_compatible_with(self, data_type: PythonType) -> bool {
        match data_type {
            PythonType::String
            | PythonType::Bytes
            | PythonType::List
            | PythonType::Dict
            | PythonType::Set
            | PythonType::Tuple
            | PythonType::Generator
            | PythonType::Ellipsis
            | PythonType::None => matches!(
                self,
                FormatType::Unknown | FormatType::String | FormatType::Repr
            ),
            PythonType::Number(NumberLike::Complex | NumberLike::Bool) => matches!(
                self,
                FormatType::Unknown | FormatType::String | FormatType::Repr
            ),
            PythonType::Number(NumberLike::Integer) => matches!(
                self,
                FormatType::Unknown
                    | FormatType::String
                    | FormatType::Repr
                    | FormatType::Integer
                    | FormatType::Float
                    | FormatType::Number
            ),
            PythonType::Number(NumberLike::Float) => matches!(
                self,
                FormatType::Unknown
                    | FormatType::String
                    | FormatType::Repr
                    | FormatType::Float
                    | FormatType::Number
            ),
        }
    }
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
    let format = FormatType::from(format.format_char);
    match ResolvedPythonType::from(value) {
        ResolvedPythonType::Atom(atom) => format.is_compatible_with(atom),
        ResolvedPythonType::Union(atoms) => {
            atoms.iter().all(|atom| format.is_compatible_with(*atom))
        }
        ResolvedPythonType::Unknown => true,
        ResolvedPythonType::TypeError => true,
    }
}

/// Return `true` if the [`Constant`] aligns with the format type.
fn is_valid_constant(formats: &[CFormatStrOrBytes<String>], value: &Expr) -> bool {
    let formats = collect_specs(formats);
    // If there is more than one format, this is not valid Python and we should
    // return true so that no error is reported.
    let [format] = formats.as_slice() else {
        return true;
    };
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
        if let Expr::Constant(ast::ExprConstant {
            value:
                Constant::Str(ast::StringConstant {
                    value: mapping_key, ..
                }),
            ..
        }) = key
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
pub(crate) fn bad_string_format_type(checker: &mut Checker, expr: &Expr, right: &Expr) {
    // Grab each string segment (in case there's an implicit concatenation).
    let content = checker.locator().slice(expr);
    let mut strings: Vec<TextRange> = vec![];
    for (tok, range) in
        lexer::lex_starts_at(content, checker.source_type.as_mode(), expr.start()).flatten()
    {
        if tok.is_string() {
            strings.push(range);
        } else if tok.is_percent() {
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
    for range in &strings {
        let string = checker.locator().slice(*range);
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
    let is_valid = match right {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => is_valid_tuple(&format_strings, elts),
        Expr::Dict(ast::ExprDict {
            keys,
            values,
            range: _,
        }) => is_valid_dict(&format_strings, keys, values),
        _ => is_valid_constant(&format_strings, right),
    };
    if !is_valid {
        checker
            .diagnostics
            .push(Diagnostic::new(BadStringFormatType, expr.range()));
    }
}
