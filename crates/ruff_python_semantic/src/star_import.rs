#[derive(Debug, Clone)]
pub struct StarImport<'a> {
    /// The level of the import. `None` or `Some(0)` indicate an absolute import.
    pub level: Option<u32>,
    /// The module being imported. `None` indicates a wildcard import.
    pub module: Option<&'a str>,
}
