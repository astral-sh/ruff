use crate::jupyter::Notebook;

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum SourceKind {
    Python(String),
    Jupyter(Notebook),
}

impl SourceKind {
    /// Return the source content.
    pub fn content(&self) -> &str {
        match self {
            SourceKind::Python(content) => content,
            SourceKind::Jupyter(notebook) => notebook.content(),
        }
    }

    /// Return the [`Notebook`] if the source kind is [`SourceKind::Jupyter`].
    pub fn notebook(&self) -> Option<&Notebook> {
        if let Self::Jupyter(notebook) = self {
            Some(notebook)
        } else {
            None
        }
    }
}
