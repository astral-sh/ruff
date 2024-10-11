use super::{ClassType, KnownClass, Type};
use crate::Db;
use rustc_hash::FxHashSet;
use std::borrow::Cow;
use std::collections::VecDeque;

pub(super) fn fork_bases<'db>(
    db: &'db dyn Db,
    bases: &[Type<'db>],
) -> FxHashSet<Box<[ClassBase<'db>]>> {
    let mut possibilities = FxHashSet::from_iter([Box::default()]);
    for base in bases {
        possibilities = add_next_base(db, &possibilities, *base);
    }
    possibilities
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MroPossibilities<'db> {
    /// It can be statically determined that there is only exactly 1
    /// possible `__mro__` for this class; here it is:
    Known(Mro<'db>),

    /// There are multiple possible `__mro__`s for this class:
    Ambiguous(FxHashSet<Option<Mro<'db>>>),
}

impl<'db> MroPossibilities<'db> {
    pub(super) fn iter<'s>(&'s self) -> MroPossibilityIterator<'s, 'db> {
        match self {
            Self::Known(single_mro) => MroPossibilityIterator::Single(std::iter::once(single_mro)),
            Self::Ambiguous(multiple_mros) => {
                MroPossibilityIterator::Multiple(multiple_mros.iter())
            }
        }
    }
}

impl<'s, 'db> IntoIterator for &'s MroPossibilities<'db> {
    type IntoIter = MroPossibilityIterator<'s, 'db>;
    type Item = Option<&'s Mro<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum ClassBase<'db> {
    Class(ClassType<'db>),
    Any,
    Todo,
    Unknown,
}

impl<'db> ClassBase<'db> {
    pub(super) fn mro_possibilities(self, db: &'db dyn Db) -> Cow<MroPossibilities<'db>> {
        match self {
            ClassBase::Class(class) => Cow::Borrowed(class.mro_possibilities(db)),
            ClassBase::Any | ClassBase::Todo | ClassBase::Unknown => {
                let object = ClassBase::Class(KnownClass::Object.to_class(db).expect_class());
                Cow::Owned(MroPossibilities::Known(Mro::from([self, object])))
            }
        }
    }

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
            // It *might* be possible to support these,
            // but it would make our logic much more complicated and less performant
            // (we'd have to consider multiple possible mros for any given class definition).
            // Neither mypy nor pyright supports these, so for now at least it seems reasonable
            // to treat these as an error.
            Type::Intersection(_) | Type::Union(_) => ClassBase::Unknown,
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

impl<'db> FromIterator<ClassBase<'db>> for Mro<'db> {
    fn from_iter<T: IntoIterator<Item = ClassBase<'db>>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'db> From<Mro<'db>> for VecDeque<ClassBase<'db>> {
    fn from(value: Mro<'db>) -> Self {
        value.0
    }
}

impl<'a, 'db> IntoIterator for &'a Mro<'db> {
    type IntoIter = std::collections::vec_deque::Iter<'a, ClassBase<'db>>;
    type Item = &'a ClassBase<'db>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'db> IntoIterator for Mro<'db> {
    type IntoIter = std::collections::vec_deque::IntoIter<ClassBase<'db>>;
    type Item = ClassBase<'db>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Implementation of the [C3-merge algorithm] for calculating a Python class's
/// [method resolution order].
///
/// [C3-merge algorithm]: https://docs.python.org/3/howto/mro.html#python-2-3-mro
/// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
pub(super) fn c3_merge(mut seqs: Vec<VecDeque<ClassBase>>) -> Option<Mro> {
    let mut mro = Mro::default();

    loop {
        seqs.retain(|seq| !seq.is_empty());

        if seqs.is_empty() {
            return Some(mro);
        }

        let mut candidate: Option<ClassBase> = None;

        for seq in &seqs {
            let maybe_candidate = seq[0];

            let is_valid_candidate = !seqs
                .iter()
                .any(|seq| seq.iter().skip(1).any(|base| base == &maybe_candidate));

            if is_valid_candidate {
                candidate = Some(maybe_candidate);
                break;
            }
        }

        let candidate = candidate?;

        mro.push(candidate);

        for seq in &mut seqs {
            if seq[0] == candidate {
                seq.pop_front();
            }
        }
    }
}
