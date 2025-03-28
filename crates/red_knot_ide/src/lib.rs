mod db;
mod find_node;
mod goto;

pub use db::Db;
pub use goto::go_to_type_definition;
use red_knot_python_semantic::types::{
    ClassLiteralType, FunctionType, InstanceType, KnownInstanceType, ModuleLiteralType, Type,
};
use ruff_db::files::{File, FileRange};
use ruff_text_size::{Ranged, TextRange};

/// Information associated with a text range.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct RangeInfo<T> {
    pub range: FileRange,
    pub info: T,
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
            Type::FunctionLiteral(function) => function.navigation_targets(db),
            Type::ModuleLiteral(module) => module.navigation_targets(db),
            Type::Union(union) => union
                .iter(db.upcast())
                .flat_map(|target| target.navigation_targets(db))
                .collect(),
            Type::ClassLiteral(class) => class.navigation_targets(db),
            Type::Instance(instance) => instance.navigation_targets(db),
            Type::KnownInstance(instance) => instance.navigation_targets(db),
            Type::StringLiteral(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
            | Type::SliceLiteral(_) => self.to_meta_type(db.upcast()).navigation_targets(db),

            Type::Dynamic(_)
            | Type::SubclassOf(_)
            | Type::Never
            | Type::Callable(_)
            | Type::Intersection(_)
            | Type::Tuple(_) => NavigationTargets::empty(),
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

impl HasNavigationTargets for ClassLiteralType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let class = self.class();
        let class_range = class.focus_range(db.upcast());
        NavigationTargets::single(NavigationTarget {
            file: class_range.file(),
            focus_range: class_range.range(),
            full_range: class.full_range(db.upcast()).range(),
        })
    }
}

impl HasNavigationTargets for InstanceType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let class = self.class();
        let class_range = class.focus_range(db.upcast());
        NavigationTargets::single(NavigationTarget {
            file: class_range.file(),
            focus_range: class_range.range(),
            full_range: class.full_range(db.upcast()).range(),
        })
    }
}

impl HasNavigationTargets for ModuleLiteralType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        let file = self.module(db).file();

        NavigationTargets::single(NavigationTarget {
            file,
            focus_range: TextRange::default(),
            full_range: TextRange::default(),
        })
    }
}

impl HasNavigationTargets for KnownInstanceType<'_> {
    fn navigation_targets(&self, db: &dyn Db) -> NavigationTargets {
        match self {
            KnownInstanceType::TypeVar(var) => {
                let range = var.range(db.upcast());
                NavigationTargets::single(NavigationTarget {
                    file: range.file(),
                    focus_range: range.range(),
                    full_range: range.range(),
                })
            }

            // TODO: Track the definition of `KnownInstance` and navigate to their definition.
            _ => NavigationTargets::empty(),
        }
    }
}
