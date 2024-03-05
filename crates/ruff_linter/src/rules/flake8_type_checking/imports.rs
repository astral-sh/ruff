use ruff_python_semantic::{AnyImport, Binding, ResolvedReferenceId};
use ruff_text_size::{Ranged, TextRange};

/// An import with its surrounding context.
pub(crate) struct ImportBinding<'a> {
    /// The qualified name of the import (e.g., `typing.List` for `from typing import List`).
    pub(crate) import: AnyImport<'a, 'a>,
    /// The binding for the imported symbol.
    pub(crate) binding: &'a Binding<'a>,
    /// The first reference to the imported symbol.
    pub(crate) reference_id: ResolvedReferenceId,
    /// The trimmed range of the import (e.g., `List` in `from typing import List`).
    pub(crate) range: TextRange,
    /// The range of the import's parent statement.
    pub(crate) parent_range: Option<TextRange>,
}

impl Ranged for ImportBinding<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
