use ruff_db::files::File;
use ruff_text_size::{Ranged, TextRange};
use std::fmt::Formatter;
use std::ops::Deref;
use std::sync::Arc;

#[derive(Debug, Eq, PartialEq)]
pub struct TypeCheckDiagnostic {
    // TODO: Don't use string keys for rules
    pub(super) rule: String,
    pub(super) message: String,
    pub(super) range: TextRange,
    pub(super) file: File,
}

impl TypeCheckDiagnostic {
    pub fn rule(&self) -> &str {
        &self.rule
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn file(&self) -> File {
        self.file
    }
}

impl Ranged for TypeCheckDiagnostic {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// A collection of type check diagnostics.
///
/// The diagnostics are wrapped in an `Arc` because they need to be cloned multiple times
/// when going from `infer_expression` to `check_file`. We could consider
/// making [`TypeCheckDiagnostic`] a Salsa struct to have them Arena-allocated (once the Tables refactor is done).
/// Using Salsa struct does have the downside that it leaks the Salsa dependency into diagnostics and
/// each Salsa-struct comes with an overhead.
#[derive(Default, Eq, PartialEq)]
pub struct TypeCheckDiagnostics {
    inner: Vec<std::sync::Arc<TypeCheckDiagnostic>>,
}

impl TypeCheckDiagnostics {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub(super) fn push(&mut self, diagnostic: TypeCheckDiagnostic) {
        self.inner.push(Arc::new(diagnostic));
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        self.inner.shrink_to_fit();
    }
}

impl Extend<TypeCheckDiagnostic> for TypeCheckDiagnostics {
    fn extend<T: IntoIterator<Item = TypeCheckDiagnostic>>(&mut self, iter: T) {
        self.inner.extend(iter.into_iter().map(std::sync::Arc::new));
    }
}

impl Extend<std::sync::Arc<TypeCheckDiagnostic>> for TypeCheckDiagnostics {
    fn extend<T: IntoIterator<Item = Arc<TypeCheckDiagnostic>>>(&mut self, iter: T) {
        self.inner.extend(iter);
    }
}

impl<'a> Extend<&'a std::sync::Arc<TypeCheckDiagnostic>> for TypeCheckDiagnostics {
    fn extend<T: IntoIterator<Item = &'a Arc<TypeCheckDiagnostic>>>(&mut self, iter: T) {
        self.inner
            .extend(iter.into_iter().map(std::sync::Arc::clone));
    }
}

impl std::fmt::Debug for TypeCheckDiagnostics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl Deref for TypeCheckDiagnostics {
    type Target = [std::sync::Arc<TypeCheckDiagnostic>];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl IntoIterator for TypeCheckDiagnostics {
    type Item = Arc<TypeCheckDiagnostic>;
    type IntoIter = std::vec::IntoIter<std::sync::Arc<TypeCheckDiagnostic>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a TypeCheckDiagnostics {
    type Item = &'a Arc<TypeCheckDiagnostic>;
    type IntoIter = std::slice::Iter<'a, std::sync::Arc<TypeCheckDiagnostic>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}
