use anyhow::{bail, Result};
use libcst_native::{
    Attribute, Call, Comparison, Dict, Expr, Expression, Import, ImportAlias, ImportFrom,
    ImportNames, Module, SimpleString, SmallStatement, Statement,
};

pub fn match_module(module_text: &str) -> Result<Module> {
    match libcst_native::parse_module(module_text, None) {
        Ok(module) => Ok(module),
        Err(_) => bail!("Failed to extract CST from source"),
    }
}

pub fn match_expression(expression_text: &str) -> Result<Expression> {
    match libcst_native::parse_expression(expression_text) {
        Ok(expression) => Ok(expression),
        Err(_) => bail!("Failed to extract CST from source"),
    }
}

pub fn match_expr<'a, 'b>(module: &'a mut Module<'b>) -> Result<&'a mut Expr<'b>> {
    if let Some(Statement::Simple(expr)) = module.body.first_mut() {
        if let Some(SmallStatement::Expr(expr)) = expr.body.first_mut() {
            Ok(expr)
        } else {
            bail!("Expected SmallStatement::Expr")
        }
    } else {
        bail!("Expected Statement::Simple")
    }
}

pub fn match_import<'a, 'b>(module: &'a mut Module<'b>) -> Result<&'a mut Import<'b>> {
    if let Some(Statement::Simple(expr)) = module.body.first_mut() {
        if let Some(SmallStatement::Import(expr)) = expr.body.first_mut() {
            Ok(expr)
        } else {
            bail!("Expected SmallStatement::Import")
        }
    } else {
        bail!("Expected Statement::Simple")
    }
}

pub fn match_import_from<'a, 'b>(module: &'a mut Module<'b>) -> Result<&'a mut ImportFrom<'b>> {
    if let Some(Statement::Simple(expr)) = module.body.first_mut() {
        if let Some(SmallStatement::ImportFrom(expr)) = expr.body.first_mut() {
            Ok(expr)
        } else {
            bail!("Expected SmallStatement::ImportFrom")
        }
    } else {
        bail!("Expected Statement::Simple")
    }
}

pub fn match_aliases<'a, 'b>(
    import_from: &'a mut ImportFrom<'b>,
) -> Result<&'a mut Vec<ImportAlias<'b>>> {
    if let ImportNames::Aliases(aliases) = &mut import_from.names {
        Ok(aliases)
    } else {
        bail!("Expected ImportNames::Aliases")
    }
}

pub fn match_call<'a, 'b>(expression: &'a mut Expression<'b>) -> Result<&'a mut Call<'b>> {
    if let Expression::Call(call) = expression {
        Ok(call)
    } else {
        bail!("Expected Expression::Call")
    }
}

pub fn match_comparison<'a, 'b>(
    expression: &'a mut Expression<'b>,
) -> Result<&'a mut Comparison<'b>> {
    if let Expression::Comparison(comparison) = expression {
        Ok(comparison)
    } else {
        bail!("Expected Expression::Comparison")
    }
}

pub fn match_dict<'a, 'b>(expression: &'a mut Expression<'b>) -> Result<&'a mut Dict<'b>> {
    if let Expression::Dict(dict) = expression {
        Ok(dict)
    } else {
        bail!("Expected Expression::Dict")
    }
}

pub fn match_attribute<'a, 'b>(
    expression: &'a mut Expression<'b>,
) -> Result<&'a mut Attribute<'b>> {
    if let Expression::Attribute(attribute) = expression {
        Ok(attribute)
    } else {
        bail!("Expected Expression::Attribute")
    }
}

pub fn match_simple_string<'a, 'b>(
    expression: &'a mut Expression<'b>,
) -> Result<&'a mut SimpleString<'b>> {
    if let Expression::SimpleString(simple_string) = expression {
        Ok(simple_string)
    } else {
        bail!("Expected Expression::SimpleString")
    }
}
