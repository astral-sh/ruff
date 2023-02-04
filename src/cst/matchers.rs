use anyhow::{bail, Result};
use libcst_native::{
    Call, Comparison, Expr, Expression, Import, ImportFrom, Module, SmallStatement, Statement,
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
