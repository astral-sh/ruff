use anyhow::{bail, Ok, Result};
use libcst_native::{Codegen, CodegenState, DictElement, Expression};
use rustpython_parser::ast::{Excepthandler, Expr};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::Edit;
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::str::raw_contents;
use ruff_python_ast::types::Range;
use rustpython_common::format::{
    FieldName, FieldNamePart, FieldType, FormatPart, FormatString, FromTemplate,
};

use crate::cst::matchers::{
    match_attribute, match_call, match_dict, match_expression, match_simple_string,
};

/// Generate a [`Edit`] to remove unused keys from format dict.
pub fn remove_unused_format_arguments_from_dict(
    unused_arguments: &[&str],
    stmt: &Expr,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(stmt);
    let mut tree = match_expression(module_text)?;
    let dict = match_dict(&mut tree)?;

    dict.elements.retain(|e| {
        !matches!(e, DictElement::Simple {
            key: Expression::SimpleString(name),
            ..
        } if raw_contents(name.value).map_or(false, |name| unused_arguments.contains(&name)))
    });

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        stmt.location,
        stmt.end_location.unwrap(),
    ))
}

/// Generate a [`Edit`] to remove unused keyword arguments from a `format` call.
pub fn remove_unused_keyword_arguments_from_format_call(
    unused_arguments: &[&str],
    location: Range,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(location);
    let mut tree = match_expression(module_text)?;
    let call = match_call(&mut tree)?;

    call.args
        .retain(|e| !matches!(&e.keyword, Some(kw) if unused_arguments.contains(&kw.value)));

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        location.location,
        location.end_location,
    ))
}

fn unparse_format_part(format_part: FormatPart) -> String {
    match format_part {
        FormatPart::Literal(literal) => literal,
        FormatPart::Field {
            field_name,
            conversion_spec,
            format_spec,
        } => {
            let mut field_name = field_name;
            if let Some(conversion) = conversion_spec {
                field_name.push_str(&format!("!{conversion}"));
            }
            if !format_spec.is_empty() {
                field_name.push_str(&format!(":{format_spec}"));
            }
            format!("{{{field_name}}}")
        }
    }
}

fn update_field_types(format_string: &FormatString, min_unused: usize) -> String {
    format_string
        .format_parts
        .iter()
        .map(|part| match part {
            FormatPart::Literal(literal) => FormatPart::Literal(literal.to_string()),
            FormatPart::Field {
                field_name,
                conversion_spec,
                format_spec,
            } => {
                let new_field_name = FieldName::parse(field_name).unwrap(); // This should never fail because we parsed it before
                let mut new_field_name_string = match new_field_name.field_type {
                    FieldType::Auto => String::new(),
                    FieldType::Index(i) => (i - min_unused).to_string(),
                    FieldType::Keyword(keyword) => keyword,
                };
                for field_name_part in &new_field_name.parts {
                    let field_name_part_string = match field_name_part {
                        FieldNamePart::Attribute(attribute) => format!(".{attribute}"),
                        FieldNamePart::Index(i) => format!("[{i}]"),
                        FieldNamePart::StringIndex(s) => format!("[{s}]"),
                    };
                    new_field_name_string.push_str(&field_name_part_string);
                }
                let new_format_spec = FormatString::from_str(format_spec).unwrap(); // This should never fail because we parsed it before
                let new_format_spec_string = update_field_types(&new_format_spec, min_unused);
                FormatPart::Field {
                    field_name: new_field_name_string,
                    conversion_spec: *conversion_spec,
                    format_spec: new_format_spec_string,
                }
            }
        })
        .map(unparse_format_part)
        .collect()
}

/// Generate a [`Edit`] to remove unused positional arguments from a `format` call.
pub fn remove_unused_positional_arguments_from_format_call(
    unused_arguments: &[usize],
    location: Range,
    locator: &Locator,
    stylist: &Stylist,
    format_string: &FormatString,
) -> Result<Edit> {
    let module_text = locator.slice(location);
    let mut tree = match_expression(module_text)?;
    let call = match_call(&mut tree)?;

    let mut index = 0;
    call.args.retain(|_| {
        index += 1;
        !unused_arguments.contains(&(index - 1))
    });

    let mut min_unused_index = 0;
    for index in unused_arguments {
        if *index == min_unused_index {
            min_unused_index += 1;
        } else {
            break;
        }
    }

    let mut new_format_string;
    if min_unused_index > 0 {
        let func = match_attribute(&mut call.func)?;
        let simple_string = match_simple_string(&mut func.value)?;
        new_format_string = update_field_types(format_string, min_unused_index);
        new_format_string = format!(r#""{new_format_string}""#);
        simple_string.value = new_format_string.as_str();
    }

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        location.location,
        location.end_location,
    ))
}

/// Generate a [`Edit`] to remove the binding from an exception handler.
pub fn remove_exception_handler_assignment(
    excepthandler: &Excepthandler,
    locator: &Locator,
) -> Result<Edit> {
    let contents = locator.slice(excepthandler);
    let mut fix_start = None;
    let mut fix_end = None;

    // End of the token just before the `as` to the semicolon.
    let mut prev = None;
    for (start, tok, end) in
        lexer::lex_located(contents, Mode::Module, excepthandler.location).flatten()
    {
        if matches!(tok, Tok::As) {
            fix_start = prev;
        }
        if matches!(tok, Tok::Colon) {
            fix_end = Some(start);
            break;
        }
        prev = Some(end);
    }

    if let (Some(start), Some(end)) = (fix_start, fix_end) {
        Ok(Edit::deletion(start, end))
    } else {
        bail!("Could not find span of exception handler")
    }
}
