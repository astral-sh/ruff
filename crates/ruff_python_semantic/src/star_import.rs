#[derive(Debug, Clone)]
pub struct StarImport<'a> {
    /// The level of the import. `0` indicates an absolute import.
    pub level: u32,
    /// The module being imported. `None` indicates a wildcard import.
    pub module: Option<&'a str>,
}
