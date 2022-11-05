use anyhow::Result;
use libcst_native::Module;

pub fn match_module(module_text: &str) -> Result<Module> {
    match libcst_native::parse_module(module_text, None) {
        Ok(module) => Ok(module),
        Err(_) => Err(anyhow::anyhow!("Failed to extract CST from source.")),
    }
}
