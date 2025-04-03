mod db;
mod find_node;
mod goto;
mod hover;
mod markup;

use std::ops::{Deref, DerefMut};

pub use db::Db;
pub use goto::goto_type_definition;
pub use hover::hover;
pub use markup::MarkupKind;
use red_knot_python_semantic::types::{
    Class, ClassBase, ClassLiteralType, FunctionType, InstanceType, IntersectionType,
    KnownInstanceType, ModuleLiteralType, Type,
};
use ruff_db::files::{File, FileRange};
use ruff_db::source::source_text;
use ruff_text_size::{Ranged, TextLen, TextRange};

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
#[derive(Debug, Clone)]
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
        Self(iter.into_iter().collect())
    }
}

pub trait HasNavigationTargets {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets;
}

impl HasNavigationTargets for Type<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        match self {
            Type::BoundMethod(method) => method.function(db).navigation_targets(db),
            Type::FunctionLiteral(function) => function.navigation_targets(db),
            Type::ModuleLiteral(module) => module.navigation_targets(db),
            Type::Union(union) => union
                .iter(db.upcast())
                .flat_map(|target| target.navigation_targets(db))
                .collect(),
            Type::ClassLiteral(class) => class.navigation_targets(db),
            Type::Instance(instance) => instance.navigation_targets(db),
            Type::KnownInstance(instance) => instance.navigation_targets(db),
            Type::SubclassOf(subclass_of_type) => match subclass_of_type.subclass_of() {
                ClassBase::Class(class) => class.navigation_targets(db),
                ClassBase::Dynamic(_) => NavigationTargets::empty(),
            },

            Type::StringLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::LiteralString
            | Type::IntLiteral(_)
            | Type::BytesLiteral(_)
            | Type::SliceLiteral(_)
            | Type::MethodWrapper(_)
            | Type::WrapperDescriptor(_)
            | Type::PropertyInstance(_)
            | Type::Tuple(_) => self.to_meta_type(db.upcast()).navigation_targets(db),

            Type::Intersection(intersection) => intersection.navigation_targets(db),

            Type::Dynamic(_)
            | Type::Never
            | Type::Callable(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy => NavigationTargets::empty(),
        }
    }
}

impl HasNavigationTargets for FunctionType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let function_range = self.focus_range(db.upcast());
        NavigationTargets::single(NavigationTarget {
            file: function_range.file(),
            focus_range: function_range.range(),
            full_range: self.full_range(db.upcast()).range(),
        })
    }
}

impl HasNavigationTargets for Class<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let class_range = self.focus_range(db.upcast());
        NavigationTargets::single(NavigationTarget {
            file: class_range.file(),
            focus_range: class_range.range(),
            full_range: self.full_range(db.upcast()).range(),
        })
    }
}

impl HasNavigationTargets for ClassLiteralType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        self.class().navigation_targets(db)
    }
}

impl HasNavigationTargets for InstanceType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        self.class().navigation_targets(db)
    }
}

impl HasNavigationTargets for ModuleLiteralType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let file = self.module(db).file();
        let source = source_text(db.upcast(), file);

        NavigationTargets::single(NavigationTarget {
            file,
            focus_range: TextRange::default(),
            full_range: TextRange::up_to(source.text_len()),
        })
    }
}

impl HasNavigationTargets for KnownInstanceType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        match self {
            KnownInstanceType::TypeVar(var) => {
                let definition = var.definition(db);
                let full_range = definition.full_range(db.upcast());

                NavigationTargets::single(NavigationTarget {
                    file: full_range.file(),
                    focus_range: definition.focus_range(db.upcast()).range(),
                    full_range: full_range.range(),
                })
            }

            // TODO: Track the definition of `KnownInstance` and navigate to their definition.
            _ => NavigationTargets::empty(),
        }
    }
}

impl HasNavigationTargets for IntersectionType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        // Only consider the positive elements because the negative elements are mainly from narrowing constraints.
        let mut targets = self
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
}

#[cfg(test)]
mod tests {
    use crate::db::tests::TestDb;
    use insta::internals::SettingsBindDropGuard;
    use red_knot_python_semantic::{
        Program, ProgramSettings, PythonPath, PythonPlatform, SearchPathSettings,
    };
    use ruff_db::diagnostic::{Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig};
    use ruff_db::files::{system_path_to_file, File};
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_python_ast::PythonVersion;
    use ruff_text_size::TextSize;

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
                python_version: PythonVersion::latest(),
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
                write!(buf, "{}", diag.display(&self.db, &config)).unwrap();
            }

            buf
        }
    }

    pub(super) trait IntoDiagnostic {
        fn into_diagnostic(self) -> Diagnostic;
    }
}
