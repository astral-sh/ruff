use crate::jupyter::Notebook;

pub enum SourceKind {
    Python(String),
    Jupyter(Notebook),
}

impl SourceKind {
    pub fn content(&self) -> &str {
        match self {
            SourceKind::Python(content) => content,
            SourceKind::Jupyter(notebook) => notebook.content(),
        }
    }
}
