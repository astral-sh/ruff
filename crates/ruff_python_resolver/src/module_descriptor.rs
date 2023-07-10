#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportModuleDescriptor {
    pub(crate) leading_dots: usize,
    pub(crate) name_parts: Vec<String>,
    pub(crate) imported_symbols: Vec<String>,
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
