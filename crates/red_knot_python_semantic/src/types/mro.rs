use super::{ClassType, KnownClass, Type};
use crate::Db;
use itertools::Itertools;
use rustc_hash::FxHashSet;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::iter::FusedIterator;

/// The resolved possible [method resolution order]s for a single class.
///
/// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MroPossibilities<'db> {
    /// It can be statically determined that there is only exactly 1
    /// possible `__mro__` for this class; here it is:
    Single(Mro<'db>),

    /// There are multiple possible `__mro__`s for this class:
    Multiple(FxHashSet<Option<Mro<'db>>>),
}

impl<'db> MroPossibilities<'db> {
    /// Return the possible Method Resolution Orders ("MRO"s) for this class.
    ///
    /// See [`ClassType::mro_possibilities`] for more details.
    pub(super) fn of_class(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        let bases = class.bases(db);

        // Start with some fast paths for some common occurrences:
        if !bases.iter().any(Type::is_union) {
            if let Some(short_circuit) = mro_of_class_fast_path(db, class, &bases) {
                return short_circuit;
            }
        }

        mro_of_class_slow_path(db, class, &bases)
    }

    pub(super) fn iter<'s>(&'s self) -> MroPossibilityIterator<'s, 'db> {
        match self {
            Self::Single(single_mro) => MroPossibilityIterator::Single(std::iter::once(single_mro)),
            Self::Multiple(multiple_mros) => MroPossibilityIterator::Multiple(multiple_mros.iter()),
        }
    }

    fn single(mro: impl Into<Mro<'db>>) -> Self {
        Self::Single(mro.into())
    }

    fn possibly_many(mut possibilities: FxHashSet<Option<Mro<'db>>>) -> Self {
        debug_assert_ne!(
            possibilities.len(),
            0,
            "There should always be at least one possible mro"
        );
        if possibilities.len() == 1 {
            if possibilities.iter().next().unwrap().is_some() {
                return Self::Single(possibilities.into_iter().next().unwrap().unwrap());
            }
        }
        possibilities.shrink_to_fit();
        Self::Multiple(possibilities)
    }
}

impl<'s, 'db> IntoIterator for &'s MroPossibilities<'db> {
    type IntoIter = MroPossibilityIterator<'s, 'db>;
    type Item = Option<&'s Mro<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Fast path that is only valid if we know that none of the bases is a union type
fn mro_of_class_fast_path<'db>(
    db: &'db dyn Db,
    class: ClassType<'db>,
    bases: &[Type<'db>],
) -> Option<MroPossibilities<'db>> {
    match bases {
        // 0 bases means that it must be `object` itself.
        //
        // The case for `object` itself isn't really that common,
        // but we may as well handle it here, since it's known and easy:
        [] => {
            debug_assert_eq!(
                Type::Class(class),
                KnownClass::Object.to_class(db),
                "Only `object` should have 0 bases in Python"
            );
            Some(MroPossibilities::single([class]))
        }

        // The class has a single base.
        //
        // That could be an explicit base (`class A(B): pass`),
        // or an implicit base (which is always `object`: `class A: pass`).
        [single_base] => {
            let object = KnownClass::Object.to_class(db);
            let mro = if single_base == &object {
                MroPossibilities::single([class, object.expect_class()])
            } else {
                let mut possibilities = FxHashSet::default();
                for possibility in &*ClassBase::from(single_base).mro_possibilities(db) {
                    let Some(possibility) = possibility else {
                        possibilities.insert(None);
                        continue;
                    };
                    possibilities.insert(Some(
                        std::iter::once(ClassBase::Class(class))
                            .chain(possibility.iter().copied())
                            .collect(),
                    ));
                }
                MroPossibilities::possibly_many(possibilities)
            };
            Some(mro)
        }

        // The class has multiple bases.
        //
        // At this point, whatever we do isn't really going to be "fast",
        // so we may as well fallback to the slow path below.
        // Even though we know that none of our direct bases is a union type,
        // that doesn't mean that none of our indirect bases is a union type...
        _ => None,
    }
}

/// Slow path: this is only taken if the class has multiple bases
/// (of which any might be a union type),
/// or it has a single base, and the base is a union type.
fn mro_of_class_slow_path<'db>(
    db: &'db dyn Db,
    class: ClassType<'db>,
    bases: &[Type<'db>],
) -> MroPossibilities<'db> {
    let bases_possibilities = fork_bases(db, bases);
    debug_assert_ne!(bases_possibilities.len(), 0);
    let mut mro_possibilities = FxHashSet::default();

    for bases_possibility in &bases_possibilities {
        match bases_possibility {
            [] => panic!("Only `object` should ever have 0 bases, which should have been handled in a fast path"),

            // fast path for a common case: only inherits from a single base
            [single_base] => {
                let object = ClassBase::Class(KnownClass::Object.to_class(db).expect_class());
                if *single_base == object {
                    mro_possibilities.insert(Some(Mro::from([ClassBase::Class(class), object])));
                } else {
                    for possibility in &*single_base.mro_possibilities(db) {
                        let Some(possibility) = possibility else {
                            mro_possibilities.insert(None);
                            continue;
                        };
                        mro_possibilities.insert(Some(
                            std::iter::once(ClassBase::Class(class))
                                .chain(possibility.iter().copied())
                                .collect(),
                        ));
                    }
                }
            }

            // slow path of the slow path: fall back to full C3 linearisation algorithm
            // as described in https://docs.python.org/3/howto/mro.html#python-2-3-mro
            //
            // For a Python-3 translation of the algorithm described in that document,
            // see https://gist.github.com/AlexWaygood/674db1fce6856a90f251f63e73853639
            _ => {
                let bases = VecDeque::from_iter(bases_possibility);

                let possible_mros_per_base: Vec<_> = bases
                    .iter()
                    .map(|base| base.mro_possibilities(db))
                    .collect();

                let mro_cartesian_product = possible_mros_per_base
                    .iter()
                    .map(|mro_set| mro_set.iter())
                    .multi_cartesian_product();

                // Each `possible_mros_of_bases` is a concrete possibility of the list of mros of all of the bases:
                // where the bases are `[B1, B2, B..N]`, `possible_mros_of_bases` represents one possibility of
                // `[mro_of_B1, mro_of_B2, mro_of_B..N]`
                for possible_mros_of_bases in mro_cartesian_product {
                    let Some(possible_mros_of_bases) = possible_mros_of_bases
                        .into_iter()
                        .map(|maybe_mro| maybe_mro.map(|mro|mro.iter().copied().collect()))
                        .collect::<Option<Vec<_>>>()
                    else {
                        mro_possibilities.insert(None);
                        continue;
                    };
                    let linearized = c3_merge(
                        std::iter::once(VecDeque::from([ClassBase::Class(class)]))
                            .chain(possible_mros_of_bases)
                            .chain(std::iter::once(bases.iter().copied().copied().collect()))
                            .collect(),
                    );
                    mro_possibilities.insert(linearized);
                }
            }
        }
    }

    MroPossibilities::possibly_many(mro_possibilities)
}

/// Given a list of types representing the bases of a class,
/// of which one or more types could be a [`Type::Union`] variant,
/// resolve the list into a "union of bases lists", where each list in the union
/// is guaranteed not to hold any bases that are a [`Type::Union`].
fn fork_bases<'db>(db: &'db dyn Db, bases: &[Type<'db>]) -> BasesPossibilities<'db> {
    if !bases.iter().any(Type::is_union) {
        return BasesPossibilities::Single(bases.iter().map(ClassBase::from).collect());
    }
    let mut possibilities = FxHashSet::from_iter([Box::default()]);
    for base in bases {
        possibilities = add_next_base(db, &possibilities, *base);
    }
    BasesPossibilities::Multiple(possibilities)
}

fn add_next_base<'db>(
    db: &'db dyn Db,
    bases_possibilities: &FxHashSet<Box<[ClassBase<'db>]>>,
    next_base: Type<'db>,
) -> FxHashSet<Box<[ClassBase<'db>]>> {
    let mut new_possibilities = FxHashSet::default();
    let mut add_non_union_base = |fork: &[ClassBase<'db>], base: Type<'db>| {
        new_possibilities.insert(
            fork.iter()
                .copied()
                .chain(std::iter::once(ClassBase::from(base)))
                .collect(),
        );
    };
    match next_base {
        Type::Union(union) => {
            for element in union.elements(db) {
                for existing_possibility in bases_possibilities {
                    add_non_union_base(existing_possibility, *element);
                }
            }
        }
        _ => {
            for possibility in bases_possibilities {
                add_non_union_base(possibility, next_base);
            }
        }
    }
    debug_assert_ne!(new_possibilities.len(), 0);
    new_possibilities
}

/// The possible value of `__bases__` for a given class.
///
/// Whereas [`ClassType::bases`] returns a list of types in which any type
/// might be a [`Type::Union`], this enum transforms the list of types so that we
/// have a union of possible `__bases__` lists rather than a single list
/// that could contain a union.
enum BasesPossibilities<'db> {
    /// There is only one possible value for the class's `__bases__`; here it is
    Single(Box<[ClassBase<'db>]>),

    /// There are multiple possible values for the class's `__bases__` tuple
    Multiple(FxHashSet<Box<[ClassBase<'db>]>>),
}

impl<'db> BasesPossibilities<'db> {
    fn len(&self) -> usize {
        match self {
            Self::Single(_) => 1,
            Self::Multiple(possibilities) => possibilities.len(),
        }
    }

    fn iter<'s>(&'s self) -> BasesPossibilityIterator<'s, 'db> {
        match self {
            Self::Single(bases) => BasesPossibilityIterator::Single(std::iter::once(&**bases)),
            Self::Multiple(bases) => BasesPossibilityIterator::Multiple(bases.iter()),
        }
    }
}

impl<'a, 'db> IntoIterator for &'a BasesPossibilities<'db> {
    type IntoIter = BasesPossibilityIterator<'a, 'db>;
    type Item = &'a [ClassBase<'db>];

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

enum BasesPossibilityIterator<'a, 'db> {
    Single(std::iter::Once<&'a [ClassBase<'db>]>),
    Multiple(std::collections::hash_set::Iter<'a, Box<[ClassBase<'db>]>>),
}

impl<'a, 'db> Iterator for BasesPossibilityIterator<'a, 'db> {
    type Item = &'a [ClassBase<'db>];

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Single(iter) => iter.next(),
            Self::Multiple(iter) => iter.next().map(Box::as_ref),
        }
    }
}

impl FusedIterator for BasesPossibilityIterator<'_, '_> {}

#[derive(Clone)]
pub(super) enum MroPossibilityIterator<'a, 'db> {
    Single(std::iter::Once<&'a Mro<'db>>),
    Multiple(std::collections::hash_set::Iter<'a, Option<Mro<'db>>>),
}

impl<'a, 'db> Iterator for MroPossibilityIterator<'a, 'db> {
    type Item = Option<&'a Mro<'db>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Single(iter) => iter.next().map(Some),
            Self::Multiple(iter) => iter.next().map(Option::as_ref),
        }
    }
}

impl FusedIterator for MroPossibilityIterator<'_, '_> {}

/// Enumeration of the possible kinds of types we allow in class bases.
///
/// This is much more limited than the [`Type`] enum:
/// all types that would be invalid to have as a class base are
/// transformed into [`ClassBase::Unknown`]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum ClassBase<'db> {
    Class(ClassType<'db>),
    Any,
    Todo,
    Unknown,
}

impl<'db> ClassBase<'db> {
    pub(super) fn own_class_member(self, db: &'db dyn Db, member: &str) -> Type<'db> {
        match self {
            Self::Any => Type::Any,
            Self::Todo => Type::Todo,
            Self::Unknown => Type::Unknown,
            Self::Class(class) => class.own_class_member(db, member),
        }
    }

    pub(super) fn display(self, db: &'db dyn Db) -> String {
        match self {
            Self::Any => "ClassBase(Any)".to_string(),
            Self::Todo => "ClassBase(Todo)".to_string(),
            Self::Unknown => "ClassBase(Unknown)".to_string(),
            Self::Class(class) => format!("ClassBase(<class '{}'>)", class.name(db)),
        }
    }

    fn mro_possibilities(self, db: &'db dyn Db) -> Cow<MroPossibilities<'db>> {
        match self {
            ClassBase::Class(class) => Cow::Borrowed(class.mro_possibilities(db)),
            ClassBase::Any | ClassBase::Todo | ClassBase::Unknown => {
                let object = ClassBase::Class(KnownClass::Object.to_class(db).expect_class());
                Cow::Owned(MroPossibilities::single([self, object]))
            }
        }
    }
}

impl<'db> From<Type<'db>> for ClassBase<'db> {
    fn from(value: Type<'db>) -> Self {
        match value {
            Type::Any => ClassBase::Any,
            Type::Todo => ClassBase::Todo,
            Type::Unknown => ClassBase::Unknown,
            Type::Class(class) => ClassBase::Class(class),
            // TODO support `__mro_entries__`?? --Alex
            Type::Instance(_) => ClassBase::Todo,
            // TODO: ??
            Type::Intersection(_) => ClassBase::Todo,
            Type::Union(_) => {
                panic!(
                    "Should never call `ClassBase::from` on a `Type::Union` variant; \
                    unions have custom handling throughout"
                )
            }
            // These are all errors:
            Type::Unbound
            | Type::BooleanLiteral(_)
            | Type::BytesLiteral(_)
            | Type::Function(_)
            | Type::IntLiteral(_)
            | Type::LiteralString
            | Type::Module(_)
            | Type::Never
            | Type::None
            | Type::StringLiteral(_)
            | Type::Tuple(_) => ClassBase::Unknown,
        }
    }
}

impl<'db> From<&Type<'db>> for ClassBase<'db> {
    fn from(value: &Type<'db>) -> Self {
        Self::from(*value)
    }
}

impl<'db> From<ClassBase<'db>> for Type<'db> {
    fn from(value: ClassBase<'db>) -> Self {
        match value {
            ClassBase::Class(class) => Type::Class(class),
            ClassBase::Any => Type::Any,
            ClassBase::Todo => Type::Todo,
            ClassBase::Unknown => Type::Unknown,
        }
    }
}

impl<'db> From<&ClassBase<'db>> for Type<'db> {
    fn from(value: &ClassBase<'db>) -> Self {
        Self::from(*value)
    }
}

/// A single possible method resolution order of a given class.
///
/// See [`ClassType::mro_possibilities`] for more details.
#[derive(PartialEq, Eq, Default, Hash, Clone, Debug)]
pub(super) struct Mro<'db>(VecDeque<ClassBase<'db>>);

impl<'db> Mro<'db> {
    pub(super) fn iter(&self) -> std::collections::vec_deque::Iter<'_, ClassBase<'db>> {
        self.0.iter()
    }

    pub(super) fn display(&self, db: &'db dyn Db) -> Vec<String> {
        self.0.iter().map(|base| base.display(db)).collect()
    }

    fn push(&mut self, element: ClassBase<'db>) {
        self.0.push_back(element);
    }
}

impl<'db, const N: usize> From<[ClassBase<'db>; N]> for Mro<'db> {
    fn from(value: [ClassBase<'db>; N]) -> Self {
        Self(VecDeque::from(value))
    }
}

impl<'db, const N: usize> From<[ClassType<'db>; N]> for Mro<'db> {
    fn from(value: [ClassType<'db>; N]) -> Self {
        value.into_iter().map(ClassBase::Class).collect()
    }
}

impl<'db> FromIterator<ClassBase<'db>> for Mro<'db> {
    fn from_iter<T: IntoIterator<Item = ClassBase<'db>>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'a, 'db> IntoIterator for &'a Mro<'db> {
    type IntoIter = std::collections::vec_deque::Iter<'a, ClassBase<'db>>;
    type Item = &'a ClassBase<'db>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Implementation of the [C3-merge algorithm] for calculating a Python class's
/// [method resolution order].
///
/// [C3-merge algorithm]: https://docs.python.org/3/howto/mro.html#python-2-3-mro
/// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
fn c3_merge(mut sequences: Vec<VecDeque<ClassBase>>) -> Option<Mro> {
    let mut mro = Mro::default();

    loop {
        sequences.retain(|sequence| !sequence.is_empty());

        if sequences.is_empty() {
            return Some(mro);
        }

        let mro_entry = sequences
            .iter()
            .map(|sequence| sequence[0])
            .find(|candidate| {
                sequences
                    .iter()
                    .all(|sequence| sequence.iter().skip(1).all(|base| base != candidate))
            })?;

        mro.push(mro_entry);

        for sequence in &mut sequences {
            if sequence[0] == mro_entry {
                sequence.pop_front();
            }
        }
    }
}
