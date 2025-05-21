mod completion;
mod db;
mod find_node;
mod goto;
mod hover;
mod inlay_hints;
mod markup;

pub use completion::completion;
pub use db::Db;
pub use goto::goto_type_definition;
pub use hover::hover;
pub use inlay_hints::inlay_hints;
pub use markup::MarkupKind;

use ruff_db::files::{File, FileRange};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use std::ops::{Deref, DerefMut};
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

#[derive(Debug, Clone)]
pub struct NavigationTargets(smallvec::SmallVec<[NavigationTarget; 1]>);

impl NavigationTargets {
    fn single(target: NavigationTarget) -> Self {
        Self(smallvec::smallvec![target])
    }

    fn empty() -> Self {
        Self(smallvec::SmallVec::new())
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
                .iter(db.upcast())
                .flat_map(|target| target.navigation_targets(db))
                .collect(),

            Type::Intersection(intersection) => {
                // Only consider the positive elements because the negative elements are mainly from narrowing constraints.
                let mut targets = intersection
                    .iter_positive(db.upcast())
                    .filter(|ty| !ty.is_unknown());

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
                .definition(db.upcast())
                .map(|definition| definition.navigation_targets(db))
                .unwrap_or_else(NavigationTargets::empty),
        }
    }
}

impl HasNavigationTargets for TypeDefinition<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let Some(full_range) = self.full_range(db.upcast()) else {
            return NavigationTargets::empty();
        };

        NavigationTargets::single(NavigationTarget {
            file: full_range.file(),
            focus_range: self.focus_range(db.upcast()).unwrap_or(full_range).range(),
            full_range: full_range.range(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::db::tests::TestDb;
    use insta::internals::SettingsBindDropGuard;
    use ruff_db::Upcast;
    use ruff_db::diagnostic::{Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig};
    use ruff_db::files::{File, system_path_to_file};
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_text_size::TextSize;
    use ty_python_semantic::{
        Program, ProgramSettings, PythonPath, PythonPlatform, PythonVersionWithSource,
        SearchPathSettings,
    };

    pub(super) fn cursor_test(source: &str) -> CursorTest {
        let mut db = TestDb::new();
        let cursor_offset = source.find("<CURSOR>").expect(
            "`source`` should contain a `<CURSOR>` marker, indicating the position of the cursor.",
        );

        let mut content = source[..cursor_offset].to_string();
        content.push_str(&source[cursor_offset + "<CURSOR>".len()..]);

        db.write_file("main.py", &content)
            .expect("write to memory file system to be successful");

        let file = system_path_to_file(&db, "main.py").expect("newly written file to existing");

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersionWithSource::default(),
                python_platform: PythonPlatform::default(),
                search_paths: SearchPathSettings {
                    extra_paths: vec![],
                    src_roots: vec![SystemPathBuf::from("/")],
                    custom_typeshed: None,
                    python_path: PythonPath::KnownSitePackages(vec![]),
                },
            },
        )
        .expect("Default settings to be valid");

        let mut insta_settings = insta::Settings::clone_current();
        insta_settings.add_filter(r#"\\(\w\w|\s|\.|")"#, "/$1");
        // Filter out TODO types because they are different between debug and release builds.
        insta_settings.add_filter(r"@Todo\(.+\)", "@Todo");

        let insta_settings_guard = insta_settings.bind_to_scope();

        CursorTest {
            db,
            cursor_offset: TextSize::try_from(cursor_offset)
                .expect("source to be smaller than 4GB"),
            file,
            _insta_settings_guard: insta_settings_guard,
        }
    }

    pub(super) struct CursorTest {
        pub(super) db: TestDb,
        pub(super) cursor_offset: TextSize,
        pub(super) file: File,
        _insta_settings_guard: SettingsBindDropGuard,
    }

    impl CursorTest {
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
                write!(buf, "{}", diag.display(&self.db.upcast(), &config)).unwrap();
            }

            buf
        }
    }

    pub(super) trait IntoDiagnostic {
        fn into_diagnostic(self) -> Diagnostic;
    }
}
