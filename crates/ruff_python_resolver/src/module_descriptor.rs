#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportModuleDescriptor {
    pub leading_dots: usize,
    pub name_parts: Vec<String>,
    pub imported_symbols: Vec<String>,
}

impl ImportModuleDescriptor {
    pub(crate) fn name(&self) -> String {
        format!(
            "{}{}",
            ".".repeat(self.leading_dots),
            &self.name_parts.join(".")
        )
    }
}
