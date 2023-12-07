use ruff_text_size::{Ranged, TextRange};

use crate::NodeId;

#[derive(Debug, Clone)]
pub struct StarImport<'a> {
    /// The level of the import. `None` or `Some(0)` indicate an absolute import.
    pub level: Option<u32>,
    /// The module being imported. `None` indicates a wildcard import.
    pub module: Option<&'a str>,
    /// The node ID of the import statement.
    pub node_id: NodeId,
    /// The range of the import statement.
    pub range: TextRange,
}

impl StarImport<'_> {
    /// Returns the fully-qualified name of the imported symbol.
    pub fn qualified_name(&self) -> String {
        if let Some(module) = self.module {
            // Ex) `from foo import *` -> `foo.*`
            let mut module_name =
                String::with_capacity((self.level.unwrap_or(0) as usize) + module.len() + 1 + 1);
            if let Some(level) = self.level {
                for _ in 0..level {
                    module_name.push('.');
                }
            }
            module_name.push_str(module);
            module_name.push_str(".*");
            module_name
        } else if let Some(level) = self.level {
            // Ex) `from . import *` -> `from . import *`
            let mut module_name = String::with_capacity(
                "from".len() + 1 + (level as usize) + 1 + "import".len() + 1 + 1,
            );
            module_name.push_str("from");
            module_name.push(' ');
            for _ in 0..level {
                module_name.push('.');
            }
            module_name.push(' ');
            module_name.push_str("import");
            module_name.push(' ');
            module_name.push('*');
            module_name
        } else {
            "*".to_string()
        }
    }
}

impl Ranged for StarImport<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
