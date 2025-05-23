use std::collections::VecDeque;
use std::ops::Deref;

use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;

use crate::Db;
use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::{ClassLiteral, ClassType, KnownInstanceType, Type};

/// The inferred method resolution order of a given class.
///
/// An MRO cannot contain non-specialized generic classes. (This is why [`ClassBase`] contains a
/// [`ClassType`], not a [`ClassLiteral`].) Any generic classes in a base class list are always
/// specialized — either because the class is explicitly specialized if there is a subscript
/// expression, or because we create the default specialization if there isn't.
///
/// The MRO of a non-specialized generic class can contain generic classes that are specialized
/// with a typevar from the inheriting class. When the inheriting class is specialized, the MRO of
/// the resulting generic alias will substitute those type variables accordingly. For instance, in
/// the following example, the MRO of `D[int]` includes `C[int]`, and the MRO of `D[U]` includes
/// `C[U]` (which is a generic alias, not a non-specialized generic class):
///
/// ```py
/// class C[T]: ...
/// class D[U](C[U]): ...
/// ```
///
/// See [`ClassType::iter_mro`] for more details.
#[derive(PartialEq, Eq, Clone, Debug, salsa::Update)]
pub(super) struct Mro<'db>(Box<[ClassBase<'db>]>);

impl<'db> Mro<'db> {
    /// Attempt to resolve the MRO of a given class. Because we derive the MRO from the list of
    /// base classes in the class definition, this operation is performed on a [class
    /// literal][ClassLiteral], not a [class type][ClassType]. (You can _also_ get the MRO of a
    /// class type, but this is done by first getting the MRO of the underlying class literal, and
    /// specializing each base class as needed if the class type is a generic alias.)
    ///
    /// In the event that a possible list of bases would (or could) lead to a `TypeError` being
    /// raised at runtime due to an unresolvable MRO, we infer the MRO of the class as being `[<the
    /// class in question>, Unknown, object]`. This seems most likely to reduce the possibility of
    /// cascading errors elsewhere. (For a generic class, the first entry in this fallback MRO uses
    /// the default specialization of the class's type variables.)
    ///
    /// (We emit a diagnostic warning about the runtime `TypeError` in
    /// [`super::infer::TypeInferenceBuilder::infer_region_scope`].)
    pub(super) fn of_class(
        db: &'db dyn Db,
        class_literal: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Self, MroError<'db>> {
        let class = class_literal.apply_optional_specialization(db, specialization);
        Self::of_class_impl(db, class, class_literal.explicit_bases(db), specialization)
            .map_err(|err| err.into_mro_error(db, class))
    }

    pub(super) fn from_error(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        Self::from([
            ClassBase::Class(class),
            ClassBase::unknown(),
            ClassBase::object(db),
        ])
    }

    fn of_class_impl(
        db: &'db dyn Db,
        class: ClassType<'db>,
        bases: &[Type<'db>],
        specialization: Option<Specialization<'db>>,
    ) -> Result<Self, MroErrorKind<'db>> {
        match bases {
            // `builtins.object` is the special case:
            // the only class in Python that has an MRO with length <2
            [] if class.is_object(db) => Ok(Self::from([
                // object is not generic, so the default specialization should be a no-op
                ClassBase::Class(class),
            ])),

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
            [] => {
                // e.g. `class Foo[T]: ...` implicitly has `Generic` inserted into its bases
                if class.is_generic() {
                    Ok(Self::from([
                        ClassBase::Class(class),
                        ClassBase::Generic,
                        ClassBase::object(db),
                    ]))
                } else {
                    Ok(Self::from([ClassBase::Class(class), ClassBase::object(db)]))
                }
            }

            // Fast path for a class that has only a single explicit base.
            //
            // This *could* theoretically be handled by the final branch below,
            // but it's a common case (i.e., worth optimizing for),
            // and the `c3_merge` function requires lots of allocations.
            [single_base]
                if !matches!(
                    single_base,
                    Type::GenericAlias(_)
                        | Type::KnownInstance(
                            KnownInstanceType::Generic(_) | KnownInstanceType::Protocol(_)
                        )
                ) =>
            {
                ClassBase::try_from_type(db, *single_base).map_or_else(
                    || Err(MroErrorKind::InvalidBases(Box::from([(0, *single_base)]))),
                    |single_base| {
                        if single_base.has_cyclic_mro(db) {
                            Err(MroErrorKind::InheritanceCycle)
                        } else {
                            Ok(std::iter::once(ClassBase::Class(class))
                                .chain(single_base.mro(db, specialization))
                                .collect())
                        }
                    },
                )
            }

            // The class has multiple explicit bases.
            //
            // We'll fallback to a full implementation of the C3-merge algorithm to determine
            // what MRO Python will give this class at runtime
            // (if an MRO is indeed resolvable at all!)
            original_bases => {
                let mut resolved_bases = vec![];
                let mut invalid_bases = vec![];

                for (i, base) in original_bases.iter().enumerate() {
                    // This emulates the behavior of `typing._GenericAlias.__mro_entries__` at
                    // <https://github.com/python/cpython/blob/ad42dc1909bdf8ec775b63fb22ed48ff42797a17/Lib/typing.py#L1487-L1500>.
                    //
                    // Note that emit a diagnostic for inheriting from bare (unsubscripted) `Generic` elsewhere
                    // (see `infer::TypeInferenceBuilder::check_class_definitions`),
                    // which is why we only care about `KnownInstanceType::Generic(Some(_))`,
                    // not `KnownInstanceType::Generic(None)`.
                    if let Type::KnownInstance(KnownInstanceType::Generic(Some(_))) = base {
                        if original_bases
                            .contains(&Type::KnownInstance(KnownInstanceType::Protocol(None)))
                        {
                            continue;
                        }
                        if original_bases[i + 1..]
                            .iter()
                            .any(|b| b.is_generic_alias() && b != base)
                        {
                            continue;
                        }
                        resolved_bases.push(ClassBase::Generic);
                    } else {
                        match ClassBase::try_from_type(db, *base) {
                            Some(valid_base) => resolved_bases.push(valid_base),
                            None => invalid_bases.push((i, *base)),
                        }
                    }
                }

                if !invalid_bases.is_empty() {
                    return Err(MroErrorKind::InvalidBases(invalid_bases.into_boxed_slice()));
                }

                let mut seqs = vec![VecDeque::from([ClassBase::Class(class)])];
                for base in &resolved_bases {
                    if base.has_cyclic_mro(db) {
                        return Err(MroErrorKind::InheritanceCycle);
                    }
                    seqs.push(base.mro(db, specialization).collect());
                }
                seqs.push(
                    resolved_bases
                        .iter()
                        .map(|base| base.apply_optional_specialization(db, specialization))
                        .collect(),
                );

                if let Some(mro) = c3_merge(seqs) {
                    return Ok(mro);
                }

                let mut duplicate_dynamic_bases = false;

                let duplicate_bases: Vec<DuplicateBaseError<'db>> = {
                    let mut base_to_indices: IndexMap<ClassBase<'db>, Vec<usize>, FxBuildHasher> =
                        IndexMap::default();

                    // We need to iterate over `original_bases` here rather than `resolved_bases`
                    // so that we get the correct index of the duplicate bases if there were any
                    // (`resolved_bases` may be a longer list than `original_bases`!). However, we
                    // need to use a `ClassBase` rather than a `Type` as the key type for the
                    // `base_to_indices` map so that a class such as
                    // `class Foo(Protocol[T], Protocol): ...` correctly causes us to emit a
                    // `duplicate-base` diagnostic (matching the runtime behaviour) rather than an
                    // `inconsistent-mro` diagnostic (which would be accurate -- but not nearly as
                    // precise!).
                    for (index, base) in original_bases.iter().enumerate() {
                        let Some(base) = ClassBase::try_from_type(db, *base) else {
                            continue;
                        };
                        base_to_indices.entry(base).or_default().push(index);
                    }

                    let mut errors = vec![];

                    for (base, indices) in base_to_indices {
                        let Some((first_index, later_indices)) = indices.split_first() else {
                            continue;
                        };
                        if later_indices.is_empty() {
                            continue;
                        }
                        match base {
                            ClassBase::Class(_) | ClassBase::Generic | ClassBase::Protocol => {
                                errors.push(DuplicateBaseError {
                                    duplicate_base: base,
                                    first_index: *first_index,
                                    later_indices: later_indices.iter().copied().collect(),
                                });
                            }
                            ClassBase::Dynamic(_) => duplicate_dynamic_bases = true,
                        }
                    }

                    errors
                };

                if duplicate_bases.is_empty() {
                    if duplicate_dynamic_bases {
                        Ok(Mro::from_error(db, class))
                    } else {
                        Err(MroErrorKind::UnresolvableMro {
                            bases_list: original_bases.iter().copied().collect(),
                        })
                    }
                } else {
                    Err(MroErrorKind::DuplicateBases(
                        duplicate_bases.into_boxed_slice(),
                    ))
                }
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
/// Salsa-tracked [`ClassLiteral::try_mro`] method unless it's absolutely necessary.
pub(super) struct MroIterator<'db> {
    db: &'db dyn Db,

    /// The class whose MRO we're iterating over
    class: ClassLiteral<'db>,

    /// The specialization to apply to each MRO element, if any
    specialization: Option<Specialization<'db>>,

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
    pub(super) fn new(
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        Self {
            db,
            class,
            specialization,
            first_element_yielded: false,
            subsequent_elements: None,
        }
    }

    /// Materialize the full MRO of the class.
    /// Return an iterator over that MRO which skips the first element of the MRO.
    fn full_mro_except_first_element(&mut self) -> impl Iterator<Item = ClassBase<'db>> + '_ {
        self.subsequent_elements
            .get_or_insert_with(|| {
                let mut full_mro_iter = match self.class.try_mro(self.db, self.specialization) {
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
            return Some(ClassBase::Class(
                self.class
                    .apply_optional_specialization(self.db, self.specialization),
            ));
        }
        self.full_mro_except_first_element().next()
    }
}

impl std::iter::FusedIterator for MroIterator<'_> {}

#[derive(Debug, PartialEq, Eq, salsa::Update)]
pub(super) struct MroError<'db> {
    kind: MroErrorKind<'db>,
    fallback_mro: Mro<'db>,
}

impl<'db> MroError<'db> {
    /// Construct an MRO error of kind `InheritanceCycle`.
    pub(super) fn cycle(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        MroErrorKind::InheritanceCycle.into_mro_error(db, class)
    }

    pub(super) fn is_cycle(&self) -> bool {
        matches!(self.kind, MroErrorKind::InheritanceCycle)
    }

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
#[derive(Debug, PartialEq, Eq, salsa::Update)]
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

    /// The class has one or more duplicate bases.
    /// See [`DuplicateBaseError`] for more details.
    DuplicateBases(Box<[DuplicateBaseError<'db>]>),

    /// A cycle was encountered resolving the class' bases.
    InheritanceCycle,

    /// The MRO is otherwise unresolvable through the C3-merge algorithm.
    ///
    /// See [`c3_merge`] for more details.
    UnresolvableMro { bases_list: Box<[Type<'db>]> },
}

impl<'db> MroErrorKind<'db> {
    pub(super) fn into_mro_error(self, db: &'db dyn Db, class: ClassType<'db>) -> MroError<'db> {
        MroError {
            kind: self,
            fallback_mro: Mro::from_error(db, class),
        }
    }
}

/// Error recording the fact that a class definition was found to have duplicate bases.
#[derive(Debug, PartialEq, Eq, salsa::Update)]
pub(super) struct DuplicateBaseError<'db> {
    /// The base that is duplicated in the class's bases list.
    pub(super) duplicate_base: ClassBase<'db>,
    /// The index of the first occurrence of the base in the class's bases list.
    pub(super) first_index: usize,
    /// The indices of the base's later occurrences in the class's bases list.
    pub(super) later_indices: Box<[usize]>,
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
