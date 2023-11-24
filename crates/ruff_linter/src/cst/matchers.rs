use crate::fix::codemods::CodegenStylist;
use anyhow::{bail, Result};
use libcst_native::{
    Arg, Attribute, Call, Comparison, CompoundStatement, Dict, Expression, FormattedString,
    FormattedStringContent, FormattedStringExpression, FunctionDef, GeneratorExp, If, Import,
    ImportAlias, ImportFrom, ImportNames, IndentedBlock, Lambda, ListComp, Module, Name,
    SmallStatement, Statement, Suite, Tuple, With,
};
use ruff_python_codegen::Stylist;

pub(crate) fn match_module(module_text: &str) -> Result<Module> {
    match libcst_native::parse_module(module_text, None) {
        Ok(module) => Ok(module),
        Err(_) => bail!("Failed to extract CST from source"),
    }
}

pub(crate) fn match_statement(statement_text: &str) -> Result<Statement> {
    match libcst_native::parse_statement(statement_text) {
        Ok(statement) => Ok(statement),
        Err(_) => bail!("Failed to extract statement from source"),
    }
}

pub(crate) fn match_import<'a, 'b>(statement: &'a mut Statement<'b>) -> Result<&'a mut Import<'b>> {
    if let Statement::Simple(expr) = statement {
        if let Some(SmallStatement::Import(expr)) = expr.body.first_mut() {
            Ok(expr)
        } else {
            bail!("Expected SmallStatement::Import")
        }
    } else {
        bail!("Expected Statement::Simple")
    }
}

pub(crate) fn match_import_from<'a, 'b>(
    statement: &'a mut Statement<'b>,
) -> Result<&'a mut ImportFrom<'b>> {
    if let Statement::Simple(expr) = statement {
        if let Some(SmallStatement::ImportFrom(expr)) = expr.body.first_mut() {
            Ok(expr)
        } else {
            bail!("Expected SmallStatement::ImportFrom")
        }
    } else {
        bail!("Expected Statement::Simple")
    }
}

pub(crate) fn match_aliases<'a, 'b>(
    import_from: &'a mut ImportFrom<'b>,
) -> Result<&'a mut Vec<ImportAlias<'b>>> {
    if let ImportNames::Aliases(aliases) = &mut import_from.names {
        Ok(aliases)
    } else {
        bail!("Expected ImportNames::Aliases")
    }
}

pub(crate) fn match_call<'a, 'b>(expression: &'a Expression<'b>) -> Result<&'a Call<'b>> {
    if let Expression::Call(call) = expression {
        Ok(call)
    } else {
        bail!("Expected Expression::Call")
    }
}

pub(crate) fn match_call_mut<'a, 'b>(
    expression: &'a mut Expression<'b>,
) -> Result<&'a mut Call<'b>> {
    if let Expression::Call(call) = expression {
        Ok(call)
    } else {
        bail!("Expected Expression::Call")
    }
}

pub(crate) fn match_comparison<'a, 'b>(
    expression: &'a mut Expression<'b>,
) -> Result<&'a mut Comparison<'b>> {
    if let Expression::Comparison(comparison) = expression {
        Ok(comparison)
    } else {
        bail!("Expected Expression::Comparison")
    }
}

pub(crate) fn match_dict<'a, 'b>(expression: &'a mut Expression<'b>) -> Result<&'a mut Dict<'b>> {
    if let Expression::Dict(dict) = expression {
        Ok(dict)
    } else {
        bail!("Expected Expression::Dict")
    }
}

pub(crate) fn match_attribute<'a, 'b>(
    expression: &'a mut Expression<'b>,
) -> Result<&'a mut Attribute<'b>> {
    if let Expression::Attribute(attribute) = expression {
        Ok(attribute)
    } else {
        bail!("Expected Expression::Attribute")
    }
}

pub(crate) fn match_name<'a, 'b>(expression: &'a Expression<'b>) -> Result<&'a Name<'b>> {
    if let Expression::Name(name) = expression {
        Ok(name)
    } else {
        bail!("Expected Expression::Name")
    }
}

pub(crate) fn match_arg<'a, 'b>(call: &'a Call<'b>) -> Result<&'a Arg<'b>> {
    if let Some(arg) = call.args.first() {
        Ok(arg)
    } else {
        bail!("Expected Arg")
    }
}

pub(crate) fn match_generator_exp<'a, 'b>(
    expression: &'a Expression<'b>,
) -> Result<&'a GeneratorExp<'b>> {
    if let Expression::GeneratorExp(generator_exp) = expression {
        Ok(generator_exp)
    } else {
        bail!("Expected Expression::GeneratorExp")
    }
}

pub(crate) fn match_tuple<'a, 'b>(expression: &'a Expression<'b>) -> Result<&'a Tuple<'b>> {
    if let Expression::Tuple(tuple) = expression {
        Ok(tuple)
    } else {
        bail!("Expected Expression::Tuple")
    }
}

pub(crate) fn match_list_comp<'a, 'b>(expression: &'a Expression<'b>) -> Result<&'a ListComp<'b>> {
    if let Expression::ListComp(list_comp) = expression {
        Ok(list_comp)
    } else {
        bail!("Expected Expression::ListComp")
    }
}

pub(crate) fn match_lambda<'a, 'b>(expression: &'a Expression<'b>) -> Result<&'a Lambda<'b>> {
    if let Expression::Lambda(lambda) = expression {
        Ok(lambda)
    } else {
        bail!("Expected Expression::Lambda")
    }
}

pub(crate) fn match_formatted_string<'a, 'b>(
    expression: &'a mut Expression<'b>,
) -> Result<&'a mut FormattedString<'b>> {
    if let Expression::FormattedString(formatted_string) = expression {
        Ok(formatted_string)
    } else {
        bail!("Expected Expression::FormattedString");
    }
}

pub(crate) fn match_formatted_string_expression<'a, 'b>(
    formatted_string_content: &'a mut FormattedStringContent<'b>,
) -> Result<&'a mut FormattedStringExpression<'b>> {
    if let FormattedStringContent::Expression(formatted_string_expression) =
        formatted_string_content
    {
        Ok(formatted_string_expression)
    } else {
        bail!("Expected FormattedStringContent::Expression")
    }
}

pub(crate) fn match_function_def<'a, 'b>(
    statement: &'a mut Statement<'b>,
) -> Result<&'a mut FunctionDef<'b>> {
    if let Statement::Compound(compound) = statement {
        if let CompoundStatement::FunctionDef(function_def) = compound {
            Ok(function_def)
        } else {
            bail!("Expected CompoundStatement::FunctionDef")
        }
    } else {
        bail!("Expected Statement::Compound")
    }
}

pub(crate) fn match_indented_block<'a, 'b>(
    suite: &'a mut Suite<'b>,
) -> Result<&'a mut IndentedBlock<'b>> {
    if let Suite::IndentedBlock(indented_block) = suite {
        Ok(indented_block)
    } else {
        bail!("Expected Suite::IndentedBlock")
    }
}

pub(crate) fn match_with<'a, 'b>(statement: &'a mut Statement<'b>) -> Result<&'a mut With<'b>> {
    if let Statement::Compound(compound) = statement {
        if let CompoundStatement::With(with) = compound {
            Ok(with)
        } else {
            bail!("Expected CompoundStatement::With")
        }
    } else {
        bail!("Expected Statement::Compound")
    }
}

pub(crate) fn match_if<'a, 'b>(statement: &'a mut Statement<'b>) -> Result<&'a mut If<'b>> {
    if let Statement::Compound(compound) = statement {
        if let CompoundStatement::If(if_) = compound {
            Ok(if_)
        } else {
            bail!("Expected CompoundStatement::If")
        }
    } else {
        bail!("Expected Statement::Compound")
    }
}

/// Given the source code for an expression, return the parsed [`Expression`].
///
/// If the expression is not guaranteed to be valid as a standalone expression (e.g., if it may
/// span multiple lines and/or require parentheses), use [`transform_expression`] instead.
pub(crate) fn match_expression(expression_text: &str) -> Result<Expression> {
    match libcst_native::parse_expression(expression_text) {
        Ok(expression) => Ok(expression),
        Err(_) => bail!("Failed to extract expression from source"),
    }
}

/// Run a transformation function over an expression.
///
/// Passing an expression to [`match_expression`] directly can lead to parse errors if the
/// expression is not a valid standalone expression (e.g., it was parenthesized in the original
/// source). This method instead wraps the expression in "fake" parentheses, runs the
/// transformation, then removes the "fake" parentheses.
pub(crate) fn transform_expression(
    source_code: &str,
    stylist: &Stylist,
    func: impl FnOnce(Expression) -> Result<Expression>,
) -> Result<String> {
    // Wrap the expression in parentheses.
    let source_code = format!("({source_code})");
    let expression = match_expression(&source_code)?;

    // Run the function on the expression.
    let expression = func(expression)?;

    // Codegen the expression.
    let mut source_code = expression.codegen_stylist(stylist);

    // Drop the outer parentheses.
    source_code.drain(0..1);
    source_code.drain(source_code.len() - 1..source_code.len());
    Ok(source_code)
}

/// Like [`transform_expression`], but operates on the source code of the expression, rather than
/// the parsed [`Expression`]. This _shouldn't_ exist, but does to accommodate lifetime issues.
pub(crate) fn transform_expression_text(
    source_code: &str,
    func: impl FnOnce(String) -> Result<String>,
) -> Result<String> {
    // Wrap the expression in parentheses.
    let source_code = format!("({source_code})");

    // Run the function on the expression.
    let mut transformed = func(source_code)?;

    // Drop the outer parentheses.
    transformed.drain(0..1);
    transformed.drain(transformed.len() - 1..transformed.len());
    Ok(transformed)
}
