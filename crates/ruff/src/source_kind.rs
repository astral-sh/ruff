use crate::autofix::source_map::SourceMap;
use crate::jupyter::Notebook;

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum SourceKind {
    Python(String),
    Jupyter(Notebook),
}

impl SourceKind {
    /// Return the [`Notebook`] if the source kind is [`SourceKind::Jupyter`].
    pub fn notebook(&self) -> Option<&Notebook> {
        if let Self::Jupyter(notebook) = self {
            Some(notebook)
        } else {
            None
        }
    }

    #[must_use]
    pub(crate) fn updated(&self, new_source: String, source_map: &SourceMap) -> Self {
        match self {
            SourceKind::Jupyter(notebook) => {
                let mut cloned = notebook.clone();
                cloned.update(source_map, new_source);
                SourceKind::Jupyter(cloned)
            }
            SourceKind::Python(_) => SourceKind::Python(new_source),
        }
    }

    pub fn source_code(&self) -> &str {
        match self {
            SourceKind::Python(source) => source,
            SourceKind::Jupyter(notebook) => notebook.source_code(),
        }
    }
}
