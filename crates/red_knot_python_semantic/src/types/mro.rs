use std::collections::VecDeque;
use std::ops::Deref;

use indexmap::IndexSet;
use itertools::Either;
use rustc_hash::FxHashSet;

use super::{Class, ClassLiteralType, KnownClass, KnownInstanceType, Type};
use crate::Db;

/// The inferred method resolution order of a given class.
///
/// See [`Class::iter_mro`] for more details.
#[derive(PartialEq, Eq, Clone, Debug)]
pub(super) struct Mro<'db>(Box<[ClassBase<'db>]>);

impl<'db> Mro<'db> {
    /// Attempt to resolve the MRO of a given class
    ///
    /// In the event that a possible list of bases would (or could) lead to a
    /// `TypeError` being raised at runtime due to an unresolvable MRO, we infer
    /// the MRO of the class as being `[<the class in question>, Unknown, object]`.
    /// This seems most likely to reduce the possibility of cascading errors
    /// elsewhere.
    ///
    /// (We emit a diagnostic warning about the runtime `TypeError` in
    /// [`super::infer::TypeInferenceBuilder::infer_region_scope`].)
    pub(super) fn of_class(db: &'db dyn Db, class: Class<'db>) -> Result<Self, MroError<'db>> {
        Self::of_class_impl(db, class).map_err(|error_kind| {
            let fallback_mro = Self::from([
                ClassBase::Class(class),
                ClassBase::Unknown,
                ClassBase::object(db),
            ]);
            MroError {
                kind: error_kind,
                fallback_mro,
            }
        })
    }

    fn of_class_impl(db: &'db dyn Db, class: Class<'db>) -> Result<Self, MroErrorKind<'db>> {
        let class_bases = class.explicit_bases(db);

        match class_bases {
            // `builtins.object` is the special case:
            // the only class in Python that has an MRO with length <2
            [] if class.is_known(db, KnownClass::Object) => {
                Ok(Self::from([ClassBase::Class(class)]))
            }

            // All other classes in Python have an MRO with length >=2.
            // Even if a class has no explicit base classes,
            // it will implicitly inherit from `object` at runtime;
            // `object` will appear in the class's `__bases__` list and `__mro__`:
            //
            // ```pycon
            // >>> class Foo: ...
            // ...
            // >>> Foo.__bases__
            // (<class 'object'>,)
            // >>> Foo.__mro__
            // (<class '__main__.Foo'>, <class 'object'>)
            // ```
            [] => Ok(Self::from([ClassBase::Class(class), ClassBase::object(db)])),

            // Fast path for a class that has only a single explicit base.
            //
            // This *could* theoretically be handled by the final branch below,
            // but it's a common case (i.e., worth optimizing for),
            // and the `c3_merge` function requires lots of allocations.
            [single_base] => {
                let single_base = ClassBase::try_from_ty(*single_base).ok_or(*single_base);
                single_base.map_or_else(
                    |invalid_base_ty| {
                        let bases_info = Box::from([(0, invalid_base_ty)]);
                        Err(MroErrorKind::InvalidBases(bases_info))
                    },
                    |single_base| {
                        if let ClassBase::Class(class_base) = single_base {
                            if class_is_cyclically_defined(db, class_base) {
                                return Err(MroErrorKind::CyclicClassDefinition);
                            }
                        }
                        let mro = std::iter::once(ClassBase::Class(class))
                            .chain(single_base.mro(db))
                            .collect();
                        Ok(mro)
                    },
                )
            }

            // The class has multiple explicit bases.
            //
            // We'll fallback to a full implementation of the C3-merge algorithm to determine
            // what MRO Python will give this class at runtime
            // (if an MRO is indeed resolvable at all!)
            multiple_bases => {
                if class_is_cyclically_defined(db, class) {
                    return Err(MroErrorKind::CyclicClassDefinition);
                }

                let mut valid_bases = vec![];
                let mut invalid_bases = vec![];

                for (i, base) in multiple_bases.iter().enumerate() {
                    match ClassBase::try_from_ty(*base).ok_or(*base) {
                        Ok(valid_base) => valid_bases.push(valid_base),
                        Err(invalid_base) => invalid_bases.push((i, invalid_base)),
                    }
                }

                if !invalid_bases.is_empty() {
                    return Err(MroErrorKind::InvalidBases(invalid_bases.into_boxed_slice()));
                }

                let mut seqs = vec![VecDeque::from([ClassBase::Class(class)])];
                for base in &valid_bases {
                    seqs.push(base.mro(db).collect());
                }
                seqs.push(valid_bases.iter().copied().collect());

                c3_merge(seqs).ok_or_else(|| {
                    let mut seen_bases = FxHashSet::default();
                    let mut duplicate_bases = vec![];
                    for (index, base) in valid_bases
                        .iter()
                        .enumerate()
                        .filter_map(|(index, base)| Some((index, base.into_class_literal_type()?)))
                    {
                        if !seen_bases.insert(base) {
                            duplicate_bases.push((index, base));
                        }
                    }

                    if duplicate_bases.is_empty() {
                        MroErrorKind::UnresolvableMro {
                            bases_list: valid_bases.into_boxed_slice(),
                        }
                    } else {
                        MroErrorKind::DuplicateBases(duplicate_bases.into_boxed_slice())
                    }
                })
            }
        }
    }
}

impl<'db, const N: usize> From<[ClassBase<'db>; N]> for Mro<'db> {
    fn from(value: [ClassBase<'db>; N]) -> Self {
        Self(Box::from(value))
    }
}

impl<'db> From<Vec<ClassBase<'db>>> for Mro<'db> {
    fn from(value: Vec<ClassBase<'db>>) -> Self {
        Self(value.into_boxed_slice())
    }
}

impl<'db> Deref for Mro<'db> {
    type Target = [ClassBase<'db>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'db> FromIterator<ClassBase<'db>> for Mro<'db> {
    fn from_iter<T: IntoIterator<Item = ClassBase<'db>>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Iterator that yields elements of a class's MRO.
///
/// We avoid materialising the *full* MRO unless it is actually necessary:
/// - Materialising the full MRO is expensive
/// - We need to do it for every class in the code that we're checking, as we need to make sure
///   that there are no class definitions in the code we're checking that would cause an
///   exception to be raised at runtime. But the same does *not* necessarily apply for every class
///   in third-party and stdlib dependencies: we never emit diagnostics about non-first-party code.
/// - However, we *do* need to resolve attribute accesses on classes/instances from
///   third-party and stdlib dependencies. That requires iterating over the MRO of third-party/stdlib
///   classes, but not necessarily the *whole* MRO: often just the first element is enough.
///   Luckily we know that for any class `X`, the first element of `X`'s MRO will always be `X` itself.
///   We can therefore avoid resolving the full MRO for many third-party/stdlib classes while still
///   being faithful to the runtime semantics.
///
/// Even for first-party code, where we will have to resolve the MRO for every class we encounter,
/// loading the cached MRO comes with a certain amount of overhead, so it's best to avoid calling the
/// Salsa-tracked [`Class::try_mro`] method unless it's absolutely necessary.
pub(super) struct MroIterator<'db> {
    db: &'db dyn Db,

    /// The class whose MRO we're iterating over
    class: Class<'db>,

    /// Whether or not we've already yielded the first element of the MRO
    first_element_yielded: bool,

    /// Iterator over all elements of the MRO except the first.
    ///
    /// The full MRO is expensive to materialize, so this field is `None`
    /// unless we actually *need* to iterate past the first element of the MRO,
    /// at which point it is lazily materialized.
    subsequent_elements: Option<std::slice::Iter<'db, ClassBase<'db>>>,
}

impl<'db> MroIterator<'db> {
    pub(super) fn new(db: &'db dyn Db, class: Class<'db>) -> Self {
        Self {
            db,
            class,
            first_element_yielded: false,
            subsequent_elements: None,
        }
    }

    /// Materialize the full MRO of the class.
    /// Return an iterator over that MRO which skips the first element of the MRO.
    fn full_mro_except_first_element(&mut self) -> impl Iterator<Item = ClassBase<'db>> + '_ {
        self.subsequent_elements
            .get_or_insert_with(|| {
                let mut full_mro_iter = match self.class.try_mro(self.db) {
                    Ok(mro) => mro.iter(),
                    Err(error) => error.fallback_mro().iter(),
                };
                full_mro_iter.next();
                full_mro_iter
            })
            .copied()
    }
}

impl<'db> Iterator for MroIterator<'db> {
    type Item = ClassBase<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.first_element_yielded {
            self.first_element_yielded = true;
            return Some(ClassBase::Class(self.class));
        }
        self.full_mro_except_first_element().next()
    }
}

impl std::iter::FusedIterator for MroIterator<'_> {}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct MroError<'db> {
    kind: MroErrorKind<'db>,
    fallback_mro: Mro<'db>,
}

impl<'db> MroError<'db> {
    /// Return an [`MroErrorKind`] variant describing why we could not resolve the MRO for this class.
    pub(super) fn reason(&self) -> &MroErrorKind<'db> {
        &self.kind
    }

    /// Return the fallback MRO we should infer for this class during type inference
    /// (since accurate resolution of its "true" MRO was impossible)
    pub(super) fn fallback_mro(&self) -> &Mro<'db> {
        &self.fallback_mro
    }
}

/// Possible ways in which attempting to resolve the MRO of a class might fail.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum MroErrorKind<'db> {
    /// The class inherits from one or more invalid bases.
    ///
    /// To avoid excessive complexity in our implementation,
    /// we only permit classes to inherit from class-literal types,
    /// `Todo`, `Unknown` or `Any`. Anything else results in us
    /// emitting a diagnostic.
    ///
    /// This variant records the indices and types of class bases
    /// that we deem to be invalid. The indices are the indices of nodes
    /// in the bases list of the class's [`StmtClassDef`](ruff_python_ast::StmtClassDef) node.
    /// Each index is the index of a node representing an invalid base.
    InvalidBases(Box<[(usize, Type<'db>)]>),

    /// The class inherits from itself!
    ///
    /// This is very unlikely to happen in working real-world code,
    /// but it's important to explicitly account for it.
    /// If we don't, there's a possibility of an infinite loop and a panic.
    CyclicClassDefinition,

    /// The class has one or more duplicate bases.
    ///
    /// This variant records the indices and [`Class`]es
    /// of the duplicate bases. The indices are the indices of nodes
    /// in the bases list of the class's [`StmtClassDef`](ruff_python_ast::StmtClassDef) node.
    /// Each index is the index of a node representing a duplicate base.
    DuplicateBases(Box<[(usize, Class<'db>)]>),

    /// The MRO is otherwise unresolvable through the C3-merge algorithm.
    ///
    /// See [`c3_merge`] for more details.
    UnresolvableMro { bases_list: Box<[ClassBase<'db>]> },
}

/// Enumeration of the possible kinds of types we allow in class bases.
///
/// This is much more limited than the [`Type`] enum:
/// all types that would be invalid to have as a class base are
/// transformed into [`ClassBase::Unknown`]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum ClassBase<'db> {
    Any,
    Unknown,
    Todo,
    Class(Class<'db>),
}

impl<'db> ClassBase<'db> {
    pub(super) fn display(self, db: &'db dyn Db) -> impl std::fmt::Display + 'db {
        struct Display<'db> {
            base: ClassBase<'db>,
            db: &'db dyn Db,
        }

        impl std::fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.base {
                    ClassBase::Any => f.write_str("Any"),
                    ClassBase::Todo => f.write_str("Todo"),
                    ClassBase::Unknown => f.write_str("Unknown"),
                    ClassBase::Class(class) => write!(f, "<class '{}'>", class.name(self.db)),
                }
            }
        }

        Display { base: self, db }
    }

    #[cfg(test)]
    #[track_caller]
    pub(super) fn expect_class_base(self) -> Class<'db> {
        match self {
            ClassBase::Class(class) => class,
            _ => panic!("Expected a `ClassBase::Class()` variant"),
        }
    }

    /// Return a `ClassBase` representing the class `builtins.object`
    fn object(db: &'db dyn Db) -> Self {
        KnownClass::Object
            .to_class(db)
            .into_class_literal()
            .map_or(Self::Unknown, |ClassLiteralType { class }| {
                Self::Class(class)
            })
    }

    /// Attempt to resolve `ty` into a `ClassBase`.
    ///
    /// Return `None` if `ty` is not an acceptable type for a class base.
    fn try_from_ty(ty: Type<'db>) -> Option<Self> {
        match ty {
            Type::Any => Some(Self::Any),
            Type::Unknown => Some(Self::Unknown),
            Type::Todo => Some(Self::Todo),
            Type::ClassLiteral(ClassLiteralType { class }) => Some(Self::Class(class)),
            Type::Union(_) => None, // TODO -- forces consideration of multiple possible MROs?
            Type::Intersection(_) => None, // TODO -- probably incorrect?
            Type::Instance(_) => None, // TODO -- handle `__mro_entries__`?
            Type::Never
            | Type::BooleanLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::BytesLiteral(_)
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::Tuple(_)
            | Type::SliceLiteral(_)
            | Type::ModuleLiteral(_)
            | Type::SubclassOf(_) => None,
            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::Literal => None,
                KnownInstanceType::TypeVar(_) => None,
            },
        }
    }

    fn into_class_literal_type(self) -> Option<Class<'db>> {
        match self {
            Self::Class(class) => Some(class),
            _ => None,
        }
    }

    /// Iterate over the MRO of this base
    fn mro(
        self,
        db: &'db dyn Db,
    ) -> Either<impl Iterator<Item = ClassBase<'db>>, impl Iterator<Item = ClassBase<'db>>> {
        match self {
            ClassBase::Any => Either::Left([ClassBase::Any, ClassBase::object(db)].into_iter()),
            ClassBase::Unknown => {
                Either::Left([ClassBase::Unknown, ClassBase::object(db)].into_iter())
            }
            ClassBase::Todo => Either::Left([ClassBase::Todo, ClassBase::object(db)].into_iter()),
            ClassBase::Class(class) => Either::Right(class.iter_mro(db)),
        }
    }
}

impl<'db> From<ClassBase<'db>> for Type<'db> {
    fn from(value: ClassBase<'db>) -> Self {
        match value {
            ClassBase::Any => Type::Any,
            ClassBase::Todo => Type::Todo,
            ClassBase::Unknown => Type::Unknown,
            ClassBase::Class(class) => Type::ClassLiteral(ClassLiteralType { class }),
        }
    }
}

/// Implementation of the [C3-merge algorithm] for calculating a Python class's
/// [method resolution order].
///
/// [C3-merge algorithm]: https://docs.python.org/3/howto/mro.html#python-2-3-mro
/// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
fn c3_merge(mut sequences: Vec<VecDeque<ClassBase>>) -> Option<Mro> {
    // Most MROs aren't that long...
    let mut mro = Vec::with_capacity(8);

    loop {
        sequences.retain(|sequence| !sequence.is_empty());

        if sequences.is_empty() {
            return Some(Mro::from(mro));
        }

        // If the candidate exists "deeper down" in the inheritance hierarchy,
        // we should refrain from adding it to the MRO for now. Add the first candidate
        // for which this does not hold true. If this holds true for all candidates,
        // return `None`; it will be impossible to find a consistent MRO for the class
        // with the given bases.
        let mro_entry = sequences.iter().find_map(|outer_sequence| {
            let candidate = outer_sequence[0];

            let not_head = sequences
                .iter()
                .all(|sequence| sequence.iter().skip(1).all(|base| base != &candidate));

            not_head.then_some(candidate)
        })?;

        mro.push(mro_entry);

        // Make sure we don't try to add the candidate to the MRO twice:
        for sequence in &mut sequences {
            if sequence[0] == mro_entry {
                sequence.pop_front();
            }
        }
    }
}

/// Return `true` if this class appears to be a cyclic definition,
/// i.e., it inherits either directly or indirectly from itself.
///
/// A class definition like this will fail at runtime,
/// but we must be resilient to it or we could panic.
fn class_is_cyclically_defined(db: &dyn Db, class: Class) -> bool {
    fn is_cyclically_defined_recursive<'db>(
        db: &'db dyn Db,
        class: Class<'db>,
        classes_to_watch: &mut IndexSet<Class<'db>>,
    ) -> bool {
        if !classes_to_watch.insert(class) {
            return true;
        }
        for explicit_base_class in class
            .explicit_bases(db)
            .iter()
            .copied()
            .filter_map(Type::into_class_literal)
            .map(|ClassLiteralType { class }| class)
        {
            // Each base must be considered in isolation.
            // This is due to the fact that if a class uses multiple inheritance,
            // there could easily be a situation where two bases have the same class in their MROs;
            // that isn't enough to constitute the class being cyclically defined.
            let classes_to_watch_len = classes_to_watch.len();
            if is_cyclically_defined_recursive(db, explicit_base_class, classes_to_watch) {
                return true;
            }
            classes_to_watch.truncate(classes_to_watch_len);
        }
        false
    }

    class
        .explicit_bases(db)
        .iter()
        .copied()
        .filter_map(Type::into_class_literal)
        .map(|ClassLiteralType { class }| class)
        .any(|base_class| is_cyclically_defined_recursive(db, base_class, &mut IndexSet::default()))
}
