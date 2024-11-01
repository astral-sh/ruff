use std::borrow::Cow;
use std::collections::VecDeque;
use std::ops::Deref;

use rustc_hash::FxHashSet;

use ruff_python_ast as ast;

use super::{definition_expression_ty, ClassType, KnownClass, TupleType, Type};
use crate::semantic_index::definition::Definition;
use crate::{Db, HasTy, SemanticModel};

/// The inferred method resolution order of a given class.
///
/// See [`ClassType::mro`] for more details.
#[derive(PartialEq, Eq, Default, Hash, Clone, Debug)]
pub(super) struct Mro<'db>(Box<[ClassBase<'db>]>);

impl<'db> Mro<'db> {
    /// In the event that a possible list of bases would (or could) lead to a
    /// `TypeError` being raised at runtime due to an unresolvable MRO, we
    /// infer the class as being `[<the class in question>, Unknown, object]`.
    /// This seems most likely to reduce the possibility of cascading errors
    /// elsewhere.
    ///
    /// (We emit a diagnostic warning about the runtime `TypeError` in
    /// [`super::infer::TypeInferenceBuilder::infer_region_scope`].)
    pub(super) fn from_error(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        Self::from([
            ClassBase::Class(class),
            ClassBase::Unknown,
            ClassBase::object(db),
        ])
    }

    /// Attempt to resolve the MRO of a given class
    pub(super) fn of_class(db: &'db dyn Db, class: ClassType<'db>) -> Result<Self, MroError<'db>> {
        let class_stmt_node = class.node(db);

        match class_stmt_node.bases() {
            [] if class.is_known(db, KnownClass::Object) => {
                Ok(Self::from([ClassBase::Class(class)]))
            }
            [] => Ok(Self::from([ClassBase::Class(class), ClassBase::object(db)])),
            [single_base_node] => {
                let single_base = ClassBase::try_from_node(
                    db,
                    single_base_node,
                    class_stmt_node,
                    class.definition(db),
                );
                match single_base {
                    Ok(base) => {
                        if base.into_class_literal_type() == Some(class) {
                            Err(MroError::CyclicClassDefinition(0))
                        } else {
                            let mro = std::iter::once(ClassBase::Class(class))
                                .chain(Mro::of_base(db, base).elements())
                                .collect();
                            Ok(mro)
                        }
                    }
                    Err(invalid_base_ty) => {
                        Err(MroError::InvalidBases(Box::from([(0, invalid_base_ty)])))
                    }
                }
            }
            multiple_bases => {
                let definition = class.definition(db);
                let mut valid_bases = vec![];
                let mut invalid_bases = vec![];

                for (i, base_node) in multiple_bases.iter().enumerate() {
                    match ClassBase::try_from_node(db, base_node, class_stmt_node, definition) {
                        Ok(valid_base) => {
                            if valid_base.into_class_literal_type() == Some(class) {
                                return Err(MroError::CyclicClassDefinition(i));
                            }
                            valid_bases.push(valid_base);
                        }
                        Err(invalid_base) => invalid_bases.push((i, invalid_base)),
                    }
                }

                if !invalid_bases.is_empty() {
                    return Err(MroError::InvalidBases(invalid_bases.into_boxed_slice()));
                }

                let mut seqs = vec![VecDeque::from([ClassBase::Class(class)])];
                for base in &valid_bases {
                    seqs.push(Mro::of_base(db, *base).elements().collect());
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
                        MroError::UnresolvableMro(valid_bases.into_boxed_slice())
                    } else {
                        MroError::DuplicateBases(duplicate_bases.into_boxed_slice())
                    }
                })
            }
        }
    }

    /// Iterate over the elements of the MRO
    pub(super) fn elements(&self) -> impl Iterator<Item = ClassBase<'db>> + '_ {
        self.0.iter().copied()
    }

    /// Return a [`TupleType`] instance where each element in the tuple
    /// represents an element in the class's MRO.
    pub(super) fn as_tuple_type(&self, db: &'db dyn Db) -> TupleType<'db> {
        let element_tys: Box<_> = self.elements().map(Type::from).collect();
        TupleType::new(db, element_tys)
    }

    /// Resolve the MRO of a [`ClassBase`]
    fn of_base(db: &'db dyn Db, base: ClassBase<'db>) -> Cow<'db, Self> {
        match base {
            ClassBase::Any => Cow::Owned(Mro::from([ClassBase::Any, ClassBase::object(db)])),
            ClassBase::Unknown => {
                Cow::Owned(Mro::from([ClassBase::Unknown, ClassBase::object(db)]))
            }
            ClassBase::Todo => Cow::Owned(Mro::from([ClassBase::Todo, ClassBase::object(db)])),
            ClassBase::Class(class) => class.mro(db),
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

/// Possible ways in which attempting to resolve the MRO of a class might fail.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum MroError<'db> {
    /// The class inherits from one or more invalid bases.
    ///
    /// To avoid excessive complexity in our implementation,
    /// we only permit classes to inherit from class-literal types,
    /// `Todo`, `Unknown` or `Any`. Anything else results in us
    /// emitting a diagnostic. This variant records the indices and types
    /// of class bases that we deem to be invalid.
    InvalidBases(Box<[(usize, Type<'db>)]>),

    /// The class inherits from itself!
    ///
    /// This is very unlikely to happen in real-world code,
    /// but it's important to explicitly account for it,
    /// as there's a possibility of an infinite loop and a panic
    /// if we don't.
    CyclicClassDefinition(usize),

    /// The class has one or more duplicate bases.
    ///
    /// This variant records the indices and [`ClassType`]s
    /// of the duplicate bases.
    DuplicateBases(Box<[(usize, ClassType<'db>)]>),

    /// The MRO is otherwise unresolvable through the C3-merge algorithm.
    UnresolvableMro(Box<[ClassBase<'db>]>),
}

/// Enumeration of the possible kinds of types we allow in class bases.
///
/// This is much more limited than the [`Type`] enum:
/// all types that would be invalid to have as a class base are
/// transformed into [`ClassBase::Unknown`]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(super) enum ClassBase<'db> {
    Any,
    Unknown,
    Todo,
    Class(ClassType<'db>),
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
    pub(super) fn expect_class(self) -> ClassType<'db> {
        match self {
            ClassBase::Class(class) => class,
            _ => panic!("Expected a `ClassBase::Class()` variant"),
        }
    }

    /// Return a `ClassBase` representing the class `builtins.object`
    fn object(db: &'db dyn Db) -> Self {
        KnownClass::Object
            .to_class(db)
            .into_class_literal_type()
            .map_or(Self::Unknown, Self::Class)
    }

    /// Attempt to resolve the node `base_node` into a `ClassBase`.
    ///
    /// If the inferred type of `base_node` is not an acceptable class-base type,
    /// return an error indicating what the inferred type was.
    fn try_from_node(
        db: &'db dyn Db,
        base_node: &'db ast::Expr,
        class_stmt_node: &'db ast::StmtClassDef,
        definition: Definition<'db>,
    ) -> Result<Self, Type<'db>> {
        let base_ty = if class_stmt_node.type_params.is_some() {
            // when we have a specialized scope, we'll look up the inference
            // within that scope
            let model = SemanticModel::new(db, definition.file(db));
            base_node.ty(&model)
        } else {
            // Otherwise, we can do the lookup based on the definition scope
            definition_expression_ty(db, definition, base_node)
        };

        Self::try_from_ty(base_ty).ok_or(base_ty)
    }

    fn try_from_ty(ty: Type<'db>) -> Option<Self> {
        match ty {
            Type::Any => Some(Self::Any),
            Type::Unknown => Some(Self::Unknown),
            Type::Todo => Some(Self::Todo),
            Type::ClassLiteral(class) => Some(Self::Class(class)),
            Type::Union(_) => None, // TODO -- forces consideration of multiple possible MROs?
            Type::Intersection(_) => None, // TODO -- probably incorrect?
            Type::Instance(_) => None, // TODO -- handle `__mro_entries__`?
            Type::Never
            | Type::None
            | Type::BooleanLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::BytesLiteral(_)
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::Tuple(_)
            | Type::SliceLiteral(_)
            | Type::ModuleLiteral(_) => None,
        }
    }

    fn into_class_literal_type(self) -> Option<ClassType<'db>> {
        match self {
            Self::Class(class) => Some(class),
            _ => None,
        }
    }
}

impl<'db> From<ClassBase<'db>> for Type<'db> {
    fn from(value: ClassBase<'db>) -> Self {
        match value {
            ClassBase::Any => Type::Any,
            ClassBase::Todo => Type::Todo,
            ClassBase::Unknown => Type::Unknown,
            ClassBase::Class(class) => Type::ClassLiteral(class),
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
