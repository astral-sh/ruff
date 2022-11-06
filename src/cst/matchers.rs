use anyhow::Result;
use libcst_native::{Expr, Module, SmallStatement, Statement};

pub fn match_module(module_text: &str) -> Result<Module> {
    match libcst_native::parse_module(module_text, None) {
        Ok(module) => Ok(module),
        Err(_) => Err(anyhow::anyhow!("Failed to extract CST from source.")),
    }
}

pub fn match_expr<'a, 'b>(module: &'a mut Module<'b>) -> Result<&'a mut Expr<'b>> {
    if let Some(Statement::Simple(expr)) = module.body.first_mut() {
        if let Some(SmallStatement::Expr(expr)) = expr.body.first_mut() {
            Ok(expr)
        } else {
            Err(anyhow::anyhow!("Expected node to be: SmallStatement::Expr"))
        }
    } else {
        Err(anyhow::anyhow!("Expected node to be: Statement::Simple"))
    }
}
