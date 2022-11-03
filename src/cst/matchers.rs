use anyhow::Result;
use libcst_native::Module;
use rustpython_ast::Located;

use crate::ast::types::Range;
use crate::source_code_locator::SourceCodeLocator;

pub fn match_tree<'a, T>(
    locator: &'a SourceCodeLocator,
    located: &'a Located<T>,
) -> Result<Module<'a>> {
    match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(located)),
        None,
    ) {
        Ok(module) => Ok(module),
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    }
}
