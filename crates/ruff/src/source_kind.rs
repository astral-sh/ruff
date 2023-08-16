use crate::jupyter::Notebook;

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum SourceKind {
    Python,
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
}
