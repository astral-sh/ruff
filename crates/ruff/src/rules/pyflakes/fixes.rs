use anyhow::{anyhow, bail, Ok, Result};
use libcst_native::{DictElement, Expression};
use ruff_text_size::TextRange;
use rustpython_format::{
    FieldName, FieldNamePart, FieldType, FormatPart, FormatString, FromTemplate,
};
use rustpython_parser::ast::{Excepthandler, Expr, Ranged};
use rustpython_parser::{lexer, Mode, Tok};

use crate::autofix::codemods::CodegenStylist;
use ruff_diagnostics::Edit;
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::str::{leading_quote, raw_contents, trailing_quote};

use crate::cst::matchers::{
    match_attribute, match_call_mut, match_dict, match_expression, match_simple_string,
};

/// Generate a [`Edit`] to remove unused keys from format dict.
pub(crate) fn remove_unused_format_arguments_from_dict(
    unused_arguments: &[&str],
    stmt: &Expr,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(stmt.range());
    let mut tree = match_expression(module_text)?;
    let dict = match_dict(&mut tree)?;

    dict.elements.retain(|e| {
        !matches!(e, DictElement::Simple {
            key: Expression::SimpleString(name),
            ..
        } if raw_contents(name.value).map_or(false, |name| unused_arguments.contains(&name)))
    });

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        stmt.range(),
    ))
}

/// Generate a [`Edit`] to remove unused keyword arguments from a `format` call.
pub(crate) fn remove_unused_keyword_arguments_from_format_call(
    unused_arguments: &[&str],
    location: TextRange,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(location);
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;

    call.args
        .retain(|e| !matches!(&e.keyword, Some(kw) if unused_arguments.contains(&kw.value)));

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        location,
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

fn update_field_types(format_string: &FormatString, index_map: &[usize]) -> String {
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
                // SAFETY: We've already parsed this string before.
                let new_field_name = FieldName::parse(field_name).unwrap();
                let mut new_field_name_string = match new_field_name.field_type {
                    FieldType::Auto => String::new(),
                    FieldType::Index(i) => index_map[i].to_string(),
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

                // SAFETY: We've already parsed this string before.
                let new_format_spec = FormatString::from_str(format_spec).unwrap();
                let new_format_spec_string = update_field_types(&new_format_spec, index_map);
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
pub(crate) fn remove_unused_positional_arguments_from_format_call(
    unused_arguments: &[usize],
    location: TextRange,
    locator: &Locator,
    stylist: &Stylist,
    format_string: &FormatString,
) -> Result<Edit> {
    let module_text = locator.slice(location);
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;

    // Remove any unused arguments, and generate a map from previous index to new index.
    let mut index = 0;
    let mut offset = 0;
    let mut index_map = Vec::with_capacity(call.args.len());
    call.args.retain(|_| {
        index_map.push(index - offset);
        let is_unused = unused_arguments.contains(&index);
        index += 1;
        if is_unused {
            offset += 1;
        }
        !is_unused
    });

    // If we removed an argument, we may need to rewrite the positional themselves.
    // Ex) `"{1}{2}".format(a, b, c)` to `"{0}{1}".format(b, c)`
    let rewrite_arguments = index_map
        .iter()
        .enumerate()
        .filter(|&(prev_index, _)| !unused_arguments.contains(&prev_index))
        .any(|(prev_index, &new_index)| prev_index != new_index);

    let new_format_string;
    if rewrite_arguments {
        // Extract the format string verbatim.
        let func = match_attribute(&mut call.func)?;
        let simple_string = match_simple_string(&mut func.value)?;

        // Extract existing quotes from the format string.
        let leading_quote = leading_quote(simple_string.value).ok_or_else(|| {
            anyhow!(
                "Could not find leading quote for format string: {}",
                simple_string.value
            )
        })?;
        let trailing_quote = trailing_quote(simple_string.value).ok_or_else(|| {
            anyhow!(
                "Could not find trailing quote for format string: {}",
                simple_string.value
            )
        })?;

        // Update the format string, preserving the quotes.
        new_format_string = format!(
            "{}{}{}",
            leading_quote,
            update_field_types(format_string, &index_map),
            trailing_quote
        );

        simple_string.value = new_format_string.as_str();
    }

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        location,
    ))
}

/// Generate a [`Edit`] to remove the binding from an exception handler.
pub(crate) fn remove_exception_handler_assignment(
    excepthandler: &Excepthandler,
    locator: &Locator,
) -> Result<Edit> {
    let contents = locator.slice(excepthandler.range());
    let mut fix_start = None;
    let mut fix_end = None;

    // End of the token just before the `as` to the semicolon.
    let mut prev = None;
    for (tok, range) in
        lexer::lex_starts_at(contents, Mode::Module, excepthandler.start()).flatten()
    {
        if matches!(tok, Tok::As) {
            fix_start = prev;
        }
        if matches!(tok, Tok::Colon) {
            fix_end = Some(range.start());
            break;
        }
        prev = Some(range.end());
    }

    if let (Some(start), Some(end)) = (fix_start, fix_end) {
        Ok(Edit::deletion(start, end))
    } else {
        bail!("Could not find span of exception handler")
    }
}
