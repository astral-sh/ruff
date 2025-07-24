mod completion;
mod doc_highlights;
mod docstring;
mod find_node;
mod goto;
mod goto_declaration;
mod goto_definition;
mod goto_references;
mod goto_type_definition;
mod hover;
mod inlay_hints;
mod markup;
mod references;
mod semantic_tokens;
mod signature_help;
mod stub_mapping;

pub use completion::completion;
pub use doc_highlights::document_highlights;
pub use docstring::get_parameter_documentation;
pub use goto::{goto_declaration, goto_definition, goto_type_definition};
pub use goto_references::goto_references;
pub use hover::hover;
pub use inlay_hints::inlay_hints;
pub use markup::MarkupKind;
pub use references::ReferencesMode;
pub use semantic_tokens::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens, semantic_tokens,
};
pub use signature_help::{ParameterDetails, SignatureDetails, SignatureHelpInfo, signature_help};

use ruff_db::files::{File, FileRange};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use std::ops::{Deref, DerefMut};
use ty_project::Db;
use ty_python_semantic::types::{Type, TypeDefinition};

/// Information associated with a text range.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct RangedValue<T> {
    pub range: FileRange,
    pub value: T,
}

impl<T> RangedValue<T> {
    pub fn file_range(&self) -> FileRange {
        self.range
    }
}

impl<T> Deref for RangedValue<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for RangedValue<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> IntoIterator for RangedValue<T>
where
    T: IntoIterator,
{
    type Item = T::Item;
    type IntoIter = T::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

/// Target to which the editor can navigate to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NavigationTarget {
    file: File,

    /// The range that should be focused when navigating to the target.
    ///
    /// This is typically not the full range of the node. For example, it's the range of the class's name in a class definition.
    ///
    /// The `focus_range` must be fully covered by `full_range`.
    focus_range: TextRange,

    /// The range covering the entire target.
    full_range: TextRange,
}

impl NavigationTarget {
    /// Creates a new `NavigationTarget` where the focus and full range are identical.
    pub fn new(file: File, range: TextRange) -> Self {
        Self {
            file,
            focus_range: range,
            full_range: range,
        }
    }

    pub fn file(&self) -> File {
        self.file
    }

    pub fn focus_range(&self) -> TextRange {
        self.focus_range
    }

    pub fn full_range(&self) -> TextRange {
        self.full_range
    }
}

/// Specifies the kind of reference operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceKind {
    /// A read reference to a symbol (e.g., using a variable's value)
    Read,
    /// A write reference to a symbol (e.g., assigning to a variable)
    Write,
    /// Neither a read or a write (e.g., a function or class declaration)
    Other,
}

/// Target of a reference with information about the kind of operation.
/// Unlike `NavigationTarget`, this type is specifically designed for references
/// and contains only a single range (not separate focus/full ranges) and
/// includes information about whether the reference is a read or write operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReferenceTarget {
    file_range: FileRange,
    kind: ReferenceKind,
}

impl ReferenceTarget {
    /// Creates a new `ReferenceTarget`.
    pub fn new(file: File, range: TextRange, kind: ReferenceKind) -> Self {
        Self {
            file_range: FileRange::new(file, range),
            kind,
        }
    }

    pub fn file(&self) -> File {
        self.file_range.file()
    }

    pub fn range(&self) -> TextRange {
        self.file_range.range()
    }

    pub fn file_range(&self) -> FileRange {
        self.file_range
    }

    pub fn kind(&self) -> ReferenceKind {
        self.kind
    }
}

#[derive(Debug, Clone)]
pub struct NavigationTargets(smallvec::SmallVec<[NavigationTarget; 1]>);

impl NavigationTargets {
    fn single(target: NavigationTarget) -> Self {
        Self(smallvec::smallvec_inline![target])
    }

    fn empty() -> Self {
        Self(smallvec::SmallVec::new_const())
    }

    fn unique(targets: impl IntoIterator<Item = NavigationTarget>) -> Self {
        let unique: FxHashSet<_> = targets.into_iter().collect();
        if unique.is_empty() {
            Self::empty()
        } else {
            let mut targets = unique.into_iter().collect::<Vec<_>>();
            targets.sort_by_key(|target| (target.file, target.focus_range.start()));
            Self(targets.into())
        }
    }

    fn iter(&self) -> std::slice::Iter<'_, NavigationTarget> {
        self.0.iter()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl IntoIterator for NavigationTargets {
    type Item = NavigationTarget;
    type IntoIter = smallvec::IntoIter<[NavigationTarget; 1]>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a NavigationTargets {
    type Item = &'a NavigationTarget;
    type IntoIter = std::slice::Iter<'a, NavigationTarget>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl FromIterator<NavigationTarget> for NavigationTargets {
    fn from_iter<T: IntoIterator<Item = NavigationTarget>>(iter: T) -> Self {
        Self::unique(iter)
    }
}

pub trait HasNavigationTargets {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets;
}

impl HasNavigationTargets for Type<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        match self {
            Type::Union(union) => union
                .iter(db)
                .flat_map(|target| target.navigation_targets(db))
                .collect(),

            Type::Intersection(intersection) => {
                // Only consider the positive elements because the negative elements are mainly from narrowing constraints.
                let mut targets = intersection.iter_positive(db).filter(|ty| !ty.is_unknown());

                let Some(first) = targets.next() else {
                    return NavigationTargets::empty();
                };

                match targets.next() {
                    Some(_) => {
                        // If there are multiple types in the intersection, we can't navigate to a single one
                        // because the type is the intersection of all those types.
                        NavigationTargets::empty()
                    }
                    None => first.navigation_targets(db),
                }
            }

            ty => ty
                .definition(db)
                .map(|definition| definition.navigation_targets(db))
                .unwrap_or_else(NavigationTargets::empty),
        }
    }
}

impl HasNavigationTargets for TypeDefinition<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let Some(full_range) = self.full_range(db) else {
            return NavigationTargets::empty();
        };

        NavigationTargets::single(NavigationTarget {
            file: full_range.file(),
            focus_range: self.focus_range(db).unwrap_or(full_range).range(),
            full_range: full_range.range(),
        })
    }
}

#[cfg(test)]
mod tests {
    use insta::internals::SettingsBindDropGuard;
    use ruff_db::Db;
    use ruff_db::diagnostic::{Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig};
    use ruff_db::files::{File, system_path_to_file};
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_text_size::TextSize;
    use ty_project::ProjectMetadata;
    use ty_python_semantic::{
        Program, ProgramSettings, PythonPlatform, PythonVersionWithSource, SearchPathSettings,
    };

    /// A way to create a simple single-file (named `main.py`) cursor test.
    ///
    /// Use cases that require multiple files with a `<CURSOR>` marker
    /// in a file other than `main.py` can use `CursorTest::builder()`.
    pub(super) fn cursor_test(source: &str) -> CursorTest {
        CursorTest::builder().source("main.py", source).build()
    }

    pub(super) struct CursorTest {
        pub(super) db: ty_project::TestDb,
        pub(super) cursor: Cursor,
        _insta_settings_guard: SettingsBindDropGuard,
    }

    impl CursorTest {
        pub(super) fn builder() -> CursorTestBuilder {
            CursorTestBuilder::default()
        }

        pub(super) fn write_file(
            &mut self,
            path: impl AsRef<SystemPath>,
            content: &str,
        ) -> std::io::Result<()> {
            self.db.write_file(path, content)
        }

        pub(super) fn render_diagnostics<I, D>(&self, diagnostics: I) -> String
        where
            I: IntoIterator<Item = D>,
            D: IntoDiagnostic,
        {
            use std::fmt::Write;

            let mut buf = String::new();

            let config = DisplayDiagnosticConfig::default()
                .color(false)
                .format(DiagnosticFormat::Full);
            for diagnostic in diagnostics {
                let diag = diagnostic.into_diagnostic();
                write!(buf, "{}", diag.display(&self.db, &config)).unwrap();
            }

            buf
        }
    }

    /// The file and offset into that file containing
    /// a `<CURSOR>` marker.
    pub(super) struct Cursor {
        pub(super) file: File,
        pub(super) offset: TextSize,
    }

    #[derive(Default)]
    pub(super) struct CursorTestBuilder {
        /// A list of source files, corresponding to the
        /// file's path and its contents.
        sources: Vec<Source>,
    }

    impl CursorTestBuilder {
        pub(super) fn build(&self) -> CursorTest {
            let mut db = ty_project::TestDb::new(ProjectMetadata::new(
                "test".into(),
                SystemPathBuf::from("/"),
            ));

            let mut cursor: Option<Cursor> = None;

            for &Source {
                ref path,
                ref contents,
                cursor_offset,
            } in &self.sources
            {
                db.write_file(path, contents)
                    .expect("write to memory file system to be successful");

                let file = system_path_to_file(&db, path).expect("newly written file to existing");

                if let Some(offset) = cursor_offset {
                    // This assert should generally never trip, since
                    // we have an assert on `CursorTestBuilder::source`
                    // to ensure we never have more than one marker.
                    assert!(
                        cursor.is_none(),
                        "found more than one source that contains `<CURSOR>`"
                    );
                    cursor = Some(Cursor { file, offset });
                }
            }

            let search_paths = SearchPathSettings::new(vec![SystemPathBuf::from("/")])
                .to_search_paths(db.system(), db.vendored())
                .expect("Valid search path settings");

            Program::from_settings(
                &db,
                ProgramSettings {
                    python_version: PythonVersionWithSource::default(),
                    python_platform: PythonPlatform::default(),
                    search_paths,
                },
            );

            let mut insta_settings = insta::Settings::clone_current();
            insta_settings.add_filter(r#"\\(\w\w|\s|\.|")"#, "/$1");
            // Filter out TODO types because they are different between debug and release builds.
            insta_settings.add_filter(r"@Todo\(.+\)", "@Todo");

            let insta_settings_guard = insta_settings.bind_to_scope();

            CursorTest {
                db,
                cursor: cursor.expect("at least one source to contain `<CURSOR>`"),
                _insta_settings_guard: insta_settings_guard,
            }
        }

        pub(super) fn source(
            &mut self,
            path: impl Into<SystemPathBuf>,
            contents: impl Into<String>,
        ) -> &mut CursorTestBuilder {
            const MARKER: &str = "<CURSOR>";

            let path = path.into();
            let contents = contents.into();
            let Some(cursor_offset) = contents.find(MARKER) else {
                self.sources.push(Source {
                    path,
                    contents,
                    cursor_offset: None,
                });
                return self;
            };

            if let Some(source) = self.sources.iter().find(|src| src.cursor_offset.is_some()) {
                panic!(
                    "cursor tests must contain exactly one file \
                     with a `<CURSOR>` marker, but found a marker \
                     in both `{path1}` and `{path2}`",
                    path1 = source.path,
                    path2 = path,
                );
            }

            let mut without_cursor_marker = contents[..cursor_offset].to_string();
            without_cursor_marker.push_str(&contents[cursor_offset + MARKER.len()..]);
            let cursor_offset =
                TextSize::try_from(cursor_offset).expect("source to be smaller than 4GB");
            self.sources.push(Source {
                path,
                contents: without_cursor_marker,
                cursor_offset: Some(cursor_offset),
            });
            self
        }
    }

    struct Source {
        path: SystemPathBuf,
        contents: String,
        cursor_offset: Option<TextSize>,
    }

    pub(super) trait IntoDiagnostic {
        fn into_diagnostic(self) -> Diagnostic;
    }
}
