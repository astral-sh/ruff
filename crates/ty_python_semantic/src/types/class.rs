use std::cell::RefCell;
use std::fmt::Write;
use std::sync::{LazyLock, Mutex};

use super::TypeVarVariance;
use super::{
    BoundTypeVarInstance, IntersectionBuilder, MemberLookupPolicy, Mro, MroError, MroIterator,
    SpecialFormType, SubclassOfType, Truthiness, Type, TypeQualifiers, class_base::ClassBase,
    function::FunctionType,
};
use crate::module_resolver::KnownModule;
use crate::place::TypeOrigin;
use crate::semantic_index::definition::{Definition, DefinitionState};
use crate::semantic_index::scope::{NodeWithScopeKind, Scope, ScopeKind};
use crate::semantic_index::symbol::Symbol;
use crate::semantic_index::{
    DeclarationWithConstraint, SemanticIndex, attribute_declarations, attribute_scopes,
};
use crate::types::bound_super::BoundSuperError;
use crate::types::constraints::{ConstraintSet, IteratorConstraintsExtension};
use crate::types::context::InferContext;
use crate::types::diagnostic::{INVALID_TYPE_ALIAS_TYPE, SUPER_CALL_IN_NAMED_TUPLE_METHOD};
use crate::types::enums::enum_metadata;
use crate::types::function::{DataclassTransformerParams, KnownFunction};
use crate::types::generics::{
    GenericContext, InferableTypeVars, Specialization, walk_generic_context, walk_specialization,
};
use crate::types::infer::{infer_expression_type, infer_unpack_types, nearest_enclosing_class};
use crate::types::member::{Member, class_member};
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::typed_dict::typed_dict_params_from_class_def;
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{
    ApplyTypeMappingVisitor, Binding, BoundSuperType, CallableType, CallableTypeKind,
    CallableTypes, DATACLASS_FLAGS, DataclassFlags, DataclassParams, DeprecatedInstance,
    FindLegacyTypeVarsVisitor, HasRelationToVisitor, IsDisjointVisitor, IsEquivalentVisitor,
    KnownInstanceType, ManualPEP695TypeAliasType, MaterializationKind, NormalizedVisitor,
    PropertyInstanceType, StringLiteralType, TypeAliasType, TypeContext, TypeMapping, TypeRelation,
    TypedDictParams, UnionBuilder, VarianceInferable, binding_type, declaration_type,
    determine_upper_bound,
};
use crate::{
    Db, FxIndexMap, FxIndexSet, FxOrderSet, Program,
    module_resolver::file_to_module,
    place::{
        Definedness, LookupError, LookupResult, Place, PlaceAndQualifiers, known_module_symbol,
        place_from_bindings, place_from_declarations,
    },
    semantic_index::{
        attribute_assignments,
        definition::{DefinitionKind, TargetKind},
        place_table,
        scope::ScopeId,
        semantic_index, use_def_map,
    },
    types::{
        CallArguments, CallError, CallErrorKind, MetaclassCandidate, UnionType,
        definition_expression_type,
    },
};
use indexmap::IndexSet;
use itertools::Itertools as _;
use ruff_db::diagnostic::Span;
use ruff_db::files::File;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;

fn explicit_bases_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: ClassLiteral<'db>,
) -> Box<[Type<'db>]> {
    Box::default()
}

fn inheritance_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: ClassLiteral<'db>,
) -> Option<InheritanceCycle> {
    None
}

fn implicit_attribute_initial<'db>(
    _db: &'db dyn Db,
    id: salsa::Id,
    _class_body_scope: ScopeId<'db>,
    _name: String,
    _target_method_decorator: MethodDecorator,
) -> Member<'db> {
    Member {
        inner: Place::bound(Type::divergent(id)).into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn implicit_attribute_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_member: &Member<'db>,
    member: Member<'db>,
    _class_body_scope: ScopeId<'db>,
    _name: String,
    _target_method_decorator: MethodDecorator,
) -> Member<'db> {
    let inner = member
        .inner
        .cycle_normalized(db, previous_member.inner, cycle);
    Member { inner }
}

fn try_mro_cycle_initial<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    self_: ClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
) -> Result<Mro<'db>, MroError<'db>> {
    Err(MroError::cycle(
        db,
        self_.apply_optional_specialization(db, specialization),
    ))
}

fn is_typed_dict_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: ClassLiteral<'db>,
) -> bool {
    false
}

#[allow(clippy::unnecessary_wraps)]
fn try_metaclass_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self_: ClassLiteral<'db>,
) -> Result<(Type<'db>, Option<DataclassTransformerParams<'db>>), MetaclassError<'db>> {
    Err(MetaclassError {
        kind: MetaclassErrorKind::Cycle,
    })
}

fn decorators_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: ClassLiteral<'db>,
) -> Box<[Type<'db>]> {
    Box::default()
}

fn fields_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: ClassLiteral<'db>,
    _specialization: Option<Specialization<'db>>,
    _field_policy: CodeGeneratorKind<'db>,
) -> FxIndexMap<Name, Field<'db>> {
    FxIndexMap::default()
}

/// A category of classes with code generation capabilities (with synthesized methods).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum CodeGeneratorKind<'db> {
    /// Classes decorated with `@dataclass` or similar dataclass-like decorators
    DataclassLike(Option<DataclassTransformerParams<'db>>),
    /// Classes inheriting from `typing.NamedTuple`
    NamedTuple,
    /// Classes inheriting from `typing.TypedDict`
    TypedDict,
}

impl<'db> CodeGeneratorKind<'db> {
    pub(crate) fn from_class(
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Option<Self> {
        #[salsa::tracked(cycle_initial=code_generator_of_class_initial,
            heap_size=ruff_memory_usage::heap_size
        )]
        fn code_generator_of_class<'db>(
            db: &'db dyn Db,
            class: ClassLiteral<'db>,
            specialization: Option<Specialization<'db>>,
        ) -> Option<CodeGeneratorKind<'db>> {
            if class.dataclass_params(db).is_some() {
                Some(CodeGeneratorKind::DataclassLike(None))
            } else if let Ok((_, Some(transformer_params))) = class.try_metaclass(db) {
                Some(CodeGeneratorKind::DataclassLike(Some(transformer_params)))
            } else if let Some(transformer_params) =
                class.iter_mro(db, specialization).skip(1).find_map(|base| {
                    base.into_class().and_then(|class| {
                        class.class_literal(db).0.dataclass_transformer_params(db)
                    })
                })
            {
                Some(CodeGeneratorKind::DataclassLike(Some(transformer_params)))
            } else if class
                .explicit_bases(db)
                .contains(&Type::SpecialForm(SpecialFormType::NamedTuple))
            {
                Some(CodeGeneratorKind::NamedTuple)
            } else if class.is_typed_dict(db) {
                Some(CodeGeneratorKind::TypedDict)
            } else {
                None
            }
        }

        fn code_generator_of_class_initial<'db>(
            _db: &'db dyn Db,
            _id: salsa::Id,
            _class: ClassLiteral<'db>,
            _specialization: Option<Specialization<'db>>,
        ) -> Option<CodeGeneratorKind<'db>> {
            None
        }

        code_generator_of_class(db, class, specialization)
    }

    pub(super) fn matches(
        self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> bool {
        matches!(
            (
                CodeGeneratorKind::from_class(db, class, specialization),
                self
            ),
            (Some(Self::DataclassLike(_)), Self::DataclassLike(_))
                | (Some(Self::NamedTuple), Self::NamedTuple)
                | (Some(Self::TypedDict), Self::TypedDict)
        )
    }

    pub(super) fn dataclass_transformer_params(self) -> Option<DataclassTransformerParams<'db>> {
        match self {
            Self::DataclassLike(params) => params,
            Self::NamedTuple | Self::TypedDict => None,
        }
    }
}

/// A specialization of a generic class with a particular assignment of types to typevars.
///
/// # Ordering
/// Ordering is based on the generic aliases's salsa-assigned id and not on its values.
/// The id may change between runs, or when the alias was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct GenericAlias<'db> {
    pub(crate) origin: ClassLiteral<'db>,
    pub(crate) specialization: Specialization<'db>,
}

pub(super) fn walk_generic_alias<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    alias: GenericAlias<'db>,
    visitor: &V,
) {
    walk_specialization(db, alias.specialization(db), visitor);
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for GenericAlias<'_> {}

impl<'db> GenericAlias<'db> {
    pub(super) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self::new(
            db,
            self.origin(db),
            self.specialization(db).normalized_impl(db, visitor),
        )
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            self.origin(db),
            self.specialization(db)
                .recursive_type_normalized_impl(db, div, nested)?,
        ))
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.origin(db).definition(db)
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let tcx = tcx
            .annotation
            .and_then(|ty| ty.specialization_of(db, self.origin(db)))
            .map(|specialization| specialization.types(db))
            .unwrap_or(&[]);

        Self::new(
            db,
            self.origin(db),
            self.specialization(db)
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
        )
    }

    pub(super) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        self.specialization(db)
            .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
    }

    pub(super) fn is_typed_dict(self, db: &'db dyn Db) -> bool {
        self.origin(db).is_typed_dict(db)
    }
}

impl<'db> From<GenericAlias<'db>> for Type<'db> {
    fn from(alias: GenericAlias<'db>) -> Type<'db> {
        Type::GenericAlias(alias)
    }
}

fn variance_of_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: GenericAlias<'db>,
    _typevar: BoundTypeVarInstance<'db>,
) -> TypeVarVariance {
    TypeVarVariance::Bivariant
}

#[salsa::tracked]
impl<'db> VarianceInferable<'db> for GenericAlias<'db> {
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size, cycle_initial=variance_of_cycle_initial)]
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        let origin = self.origin(db);

        let specialization = self.specialization(db);

        // if the class is the thing defining the variable, then it can
        // reference it without it being applied to the specialization
        std::iter::once(origin.variance_of(db, typevar))
            .chain(
                specialization
                    .generic_context(db)
                    .variables(db)
                    .zip(specialization.types(db))
                    .map(|(generic_typevar, ty)| {
                        if let Some(explicit_variance) =
                            generic_typevar.typevar(db).explicit_variance(db)
                        {
                            ty.with_polarity(explicit_variance).variance_of(db, typevar)
                        } else {
                            // `with_polarity` composes the passed variance with the
                            // inferred one. The inference is done lazily, as we can
                            // sometimes determine the result just from the passed
                            // variance. This operation is commutative, so we could
                            // infer either first.  We choose to make the `ClassLiteral`
                            // variance lazy, as it is known to be expensive, requiring
                            // that we traverse all members.
                            //
                            // If salsa let us look at the cache, we could check first
                            // to see if the class literal query was already run.

                            let typevar_variance_in_substituted_type = ty.variance_of(db, typevar);
                            origin
                                .with_polarity(typevar_variance_in_substituted_type)
                                .variance_of(db, generic_typevar)
                        }
                    }),
            )
            .collect()
    }
}

/// Represents a class type, which might be a non-generic class, or a specialization of a generic
/// class.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    salsa::Supertype,
    salsa::Update,
    get_size2::GetSize,
)]
pub enum ClassType<'db> {
    // `NonGeneric` is intended to mean that the `ClassLiteral` has no type parameters. There are
    // places where we currently violate this rule (e.g. so that we print `Foo` instead of
    // `Foo[Unknown]`), but most callers who need to make a `ClassType` from a `ClassLiteral`
    // should use `ClassLiteral::default_specialization` instead of assuming
    // `ClassType::NonGeneric`.
    NonGeneric(ClassLiteral<'db>),
    Generic(GenericAlias<'db>),
}

#[salsa::tracked]
impl<'db> ClassType<'db> {
    /// Return a `ClassType` representing the class `builtins.object`
    pub(super) fn object(db: &'db dyn Db) -> Self {
        KnownClass::Object
            .to_class_literal(db)
            .to_class_type(db)
            .unwrap()
    }

    pub(super) const fn is_generic(self) -> bool {
        matches!(self, Self::Generic(_))
    }

    pub(super) const fn into_generic_alias(self) -> Option<GenericAlias<'db>> {
        match self {
            Self::NonGeneric(_) => None,
            Self::Generic(generic) => Some(generic),
        }
    }

    pub(super) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        match self {
            Self::NonGeneric(_) => self,
            Self::Generic(generic) => Self::Generic(generic.normalized_impl(db, visitor)),
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::NonGeneric(_) => Some(self),
            Self::Generic(generic) => Some(Self::Generic(
                generic.recursive_type_normalized_impl(db, div, nested)?,
            )),
        }
    }

    pub(super) fn has_pep_695_type_params(self, db: &'db dyn Db) -> bool {
        match self {
            Self::NonGeneric(class) => class.has_pep_695_type_params(db),
            Self::Generic(generic) => generic.origin(db).has_pep_695_type_params(db),
        }
    }

    /// Returns the class literal and specialization for this class. For a non-generic class, this
    /// is the class itself. For a generic alias, this is the alias's origin.
    pub(crate) fn class_literal(
        self,
        db: &'db dyn Db,
    ) -> (ClassLiteral<'db>, Option<Specialization<'db>>) {
        match self {
            Self::NonGeneric(non_generic) => (non_generic, None),
            Self::Generic(generic) => (generic.origin(db), Some(generic.specialization(db))),
        }
    }

    /// Returns the class literal and specialization for this class, with an additional
    /// specialization applied if the class is generic.
    pub(crate) fn class_literal_specialized(
        self,
        db: &'db dyn Db,
        additional_specialization: Option<Specialization<'db>>,
    ) -> (ClassLiteral<'db>, Option<Specialization<'db>>) {
        match self {
            Self::NonGeneric(non_generic) => (non_generic, None),
            Self::Generic(generic) => (
                generic.origin(db),
                Some(
                    generic
                        .specialization(db)
                        .apply_optional_specialization(db, additional_specialization),
                ),
            ),
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        let (class_literal, _) = self.class_literal(db);
        class_literal.name(db)
    }

    pub(super) fn qualified_name(self, db: &'db dyn Db) -> QualifiedClassName<'db> {
        let (class_literal, _) = self.class_literal(db);
        class_literal.qualified_name(db)
    }

    pub(crate) fn known(self, db: &'db dyn Db) -> Option<KnownClass> {
        let (class_literal, _) = self.class_literal(db);
        class_literal.known(db)
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let (class_literal, _) = self.class_literal(db);
        class_literal.definition(db)
    }

    /// Return `Some` if this class is known to be a [`DisjointBase`], or `None` if it is not.
    pub(super) fn as_disjoint_base(self, db: &'db dyn Db) -> Option<DisjointBase<'db>> {
        self.class_literal(db).0.as_disjoint_base(db)
    }

    /// Return `true` if this class represents `known_class`
    pub(crate) fn is_known(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.known(db) == Some(known_class)
    }

    /// Return `true` if this class represents the builtin class `object`
    pub(crate) fn is_object(self, db: &'db dyn Db) -> bool {
        self.is_known(db, KnownClass::Object)
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            Self::NonGeneric(_) => self,
            Self::Generic(generic) => {
                Self::Generic(generic.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
        }
    }

    pub(super) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        match self {
            Self::NonGeneric(_) => {}
            Self::Generic(generic) => {
                generic.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
        }
    }

    /// Iterate over the [method resolution order] ("MRO") of the class.
    ///
    /// If the MRO could not be accurately resolved, this method falls back to iterating
    /// over an MRO that has the class directly inheriting from `Unknown`. Use
    /// [`ClassLiteral::try_mro`] if you need to distinguish between the success and failure
    /// cases rather than simply iterating over the inferred resolution order for the class.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(super) fn iter_mro(self, db: &'db dyn Db) -> MroIterator<'db> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal.iter_mro(db, specialization)
    }

    /// Iterate over the method resolution order ("MRO") of the class, optionally applying an
    /// additional specialization to it if the class is generic.
    pub(super) fn iter_mro_specialized(
        self,
        db: &'db dyn Db,
        additional_specialization: Option<Specialization<'db>>,
    ) -> MroIterator<'db> {
        let (class_literal, specialization) =
            self.class_literal_specialized(db, additional_specialization);
        class_literal.iter_mro(db, specialization)
    }

    /// Is this class final?
    pub(super) fn is_final(self, db: &'db dyn Db) -> bool {
        let (class_literal, _) = self.class_literal(db);
        class_literal.is_final(db)
    }

    /// Return `true` if `other` is present in this class's MRO.
    pub(super) fn is_subclass_of(self, db: &'db dyn Db, other: ClassType<'db>) -> bool {
        self.when_subclass_of(db, other, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    pub(super) fn when_subclass_of(
        self,
        db: &'db dyn Db,
        other: ClassType<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to_impl(
            db,
            other,
            inferable,
            TypeRelation::Subtyping,
            &HasRelationToVisitor::default(),
            &IsDisjointVisitor::default(),
        )
    }

    pub(super) fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        self.iter_mro(db).when_any(db, |base| {
            match base {
                ClassBase::Dynamic(_) => match relation {
                    TypeRelation::Subtyping
                    | TypeRelation::Redundancy
                    | TypeRelation::SubtypingAssuming(_) => {
                        ConstraintSet::from(other.is_object(db))
                    }
                    TypeRelation::Assignability => ConstraintSet::from(!other.is_final(db)),
                },

                // Protocol, Generic, and TypedDict are not represented by a ClassType.
                ClassBase::Protocol | ClassBase::Generic | ClassBase::TypedDict => {
                    ConstraintSet::from(false)
                }

                ClassBase::Class(base) => match (base, other) {
                    (ClassType::NonGeneric(base), ClassType::NonGeneric(other)) => {
                        ConstraintSet::from(base == other)
                    }
                    (ClassType::Generic(base), ClassType::Generic(other)) => {
                        ConstraintSet::from(base.origin(db) == other.origin(db)).and(db, || {
                            base.specialization(db).has_relation_to_impl(
                                db,
                                other.specialization(db),
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        })
                    }
                    (ClassType::Generic(_), ClassType::NonGeneric(_))
                    | (ClassType::NonGeneric(_), ClassType::Generic(_)) => {
                        ConstraintSet::from(false)
                    }
                },
            }
        })
    }

    pub(super) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: ClassType<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self == other {
            return ConstraintSet::from(true);
        }

        match (self, other) {
            // A non-generic class is never equivalent to a generic class.
            // Two non-generic classes are only equivalent if they are equal (handled above).
            (ClassType::NonGeneric(_), _) | (_, ClassType::NonGeneric(_)) => {
                ConstraintSet::from(false)
            }

            (ClassType::Generic(this), ClassType::Generic(other)) => {
                ConstraintSet::from(this.origin(db) == other.origin(db)).and(db, || {
                    this.specialization(db).is_equivalent_to_impl(
                        db,
                        other.specialization(db),
                        inferable,
                        visitor,
                    )
                })
            }
        }
    }

    /// Return the metaclass of this class, or `type[Unknown]` if the metaclass cannot be inferred.
    pub(super) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal
            .metaclass(db)
            .apply_optional_specialization(db, specialization)
    }

    /// Return the [`DisjointBase`] that appears first in the MRO of this class.
    ///
    /// Returns `None` if this class does not have any disjoint bases in its MRO.
    pub(super) fn nearest_disjoint_base(self, db: &'db dyn Db) -> Option<DisjointBase<'db>> {
        self.iter_mro(db)
            .filter_map(ClassBase::into_class)
            .find_map(|base| base.as_disjoint_base(db))
    }

    /// Return `true` if this class could coexist in an MRO with `other`.
    ///
    /// For two given classes `A` and `B`, it is often possible to say for sure
    /// that there could never exist any class `C` that inherits from both `A` and `B`.
    /// In these situations, this method returns `false`; in all others, it returns `true`.
    pub(super) fn could_coexist_in_mro_with(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }

        if self.is_final(db) {
            return self
                .iter_mro(db)
                .filter_map(ClassBase::into_class)
                .any(|class| class.class_literal(db).0 == other.class_literal(db).0);
        }
        if other.is_final(db) {
            return other
                .iter_mro(db)
                .filter_map(ClassBase::into_class)
                .any(|class| class.class_literal(db).0 == self.class_literal(db).0);
        }

        // Two disjoint bases can only coexist in an MRO if one is a subclass of the other.
        if self
            .nearest_disjoint_base(db)
            .is_some_and(|disjoint_base_1| {
                other
                    .nearest_disjoint_base(db)
                    .is_some_and(|disjoint_base_2| {
                        !disjoint_base_1.could_coexist_in_mro_with(db, &disjoint_base_2)
                    })
            })
        {
            return false;
        }

        // Check to see whether the metaclasses of `self` and `other` are disjoint.
        // Avoid this check if the metaclass of either `self` or `other` is `type`,
        // however, since we end up with infinite recursion in that case due to the fact
        // that `type` is its own metaclass (and we know that `type` can coexist in an MRO
        // with any other arbitrary class, anyway).
        let type_class = KnownClass::Type.to_class_literal(db);
        let self_metaclass = self.metaclass(db);
        if self_metaclass == type_class {
            return true;
        }
        let other_metaclass = other.metaclass(db);
        if other_metaclass == type_class {
            return true;
        }
        let Some(self_metaclass_instance) = self_metaclass.to_instance(db) else {
            return true;
        };
        let Some(other_metaclass_instance) = other_metaclass.to_instance(db) else {
            return true;
        };
        if self_metaclass_instance.is_disjoint_from(db, other_metaclass_instance) {
            return false;
        }

        true
    }

    /// Return a type representing "the set of all instances of the metaclass of this class".
    pub(super) fn metaclass_instance_type(self, db: &'db dyn Db) -> Type<'db> {
        self
            .metaclass(db)
            .to_instance(db)
            .expect("`Type::to_instance()` should always return `Some()` when called on the type of a metaclass")
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member on the class itself or any of its proper superclasses.
    ///
    /// TODO: Should this be made private...?
    pub(super) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal.class_member_inner(db, specialization, name, policy)
    }

    /// Returns the inferred type of the class member named `name`. Only bound members
    /// or those marked as `ClassVars` are considered.
    ///
    /// You must provide the `inherited_generic_context` that we should use for the `__new__` or
    /// `__init__` member. This is inherited from the containing class -­but importantly, from the
    /// class that the lookup is being performed on, and not the class containing the (possibly
    /// inherited) member.
    ///
    /// Returns [`Place::Undefined`] if `name` cannot be found in this class's scope
    /// directly. Use [`ClassType::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(super) fn own_class_member(
        self,
        db: &'db dyn Db,
        inherited_generic_context: Option<GenericContext<'db>>,
        name: &str,
    ) -> Member<'db> {
        fn synthesize_getitem_overload_signature<'db>(
            db: &'db dyn Db,
            index_annotation: Type<'db>,
            return_annotation: Type<'db>,
        ) -> Signature<'db> {
            let self_parameter = Parameter::positional_only(Some(Name::new_static("self")));
            let index_parameter = Parameter::positional_only(Some(Name::new_static("index")))
                .with_annotated_type(index_annotation);
            let parameters = Parameters::new(db, [self_parameter, index_parameter]);
            Signature::new(parameters, Some(return_annotation))
        }

        let (class_literal, specialization) = self.class_literal(db);

        let fallback_member_lookup = || {
            class_literal
                .own_class_member(db, inherited_generic_context, specialization, name)
                .map_type(|ty| {
                    let ty = ty.apply_optional_specialization(db, specialization);
                    match specialization.map(|spec| spec.materialization_kind(db)) {
                        Some(Some(materialization_kind)) => ty.materialize(
                            db,
                            materialization_kind,
                            &ApplyTypeMappingVisitor::default(),
                        ),
                        _ => ty,
                    }
                })
        };

        match name {
            "__len__" if class_literal.is_tuple(db) => {
                let return_type = specialization
                    .and_then(|spec| spec.tuple(db))
                    .and_then(|tuple| tuple.len().into_fixed_length())
                    .and_then(|len| i64::try_from(len).ok())
                    .map(Type::IntLiteral)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(db));

                let parameters = Parameters::new(
                    db,
                    [Parameter::positional_only(Some(Name::new_static("self")))
                        .with_annotated_type(Type::instance(db, self))],
                );

                let synthesized_dunder_method =
                    Type::function_like_callable(db, Signature::new(parameters, Some(return_type)));

                Member::definitely_declared(synthesized_dunder_method)
            }

            "__getitem__" if class_literal.is_tuple(db) => {
                specialization
                    .and_then(|spec| spec.tuple(db))
                    .map(|tuple| {
                        let mut element_type_to_indices: FxIndexMap<Type<'db>, Vec<i64>> =
                            FxIndexMap::default();

                        match tuple {
                            // E.g. for `tuple[int, str]`, we will generate the following overloads:
                            //
                            //    __getitem__(self, index: Literal[0, -2], /) -> int
                            //    __getitem__(self, index: Literal[1, -1], /) -> str
                            //
                            TupleSpec::Fixed(fixed_length_tuple) => {
                                let tuple_length = fixed_length_tuple.len();

                                for (index, ty) in fixed_length_tuple.elements().enumerate() {
                                    let entry = element_type_to_indices.entry(*ty).or_default();
                                    if let Ok(index) = i64::try_from(index) {
                                        entry.push(index);
                                    }
                                    if let Ok(index) = i64::try_from(tuple_length - index) {
                                        entry.push(0 - index);
                                    }
                                }
                            }

                            // E.g. for `tuple[str, *tuple[float, ...], bytes, range]`, we will generate the following overloads:
                            //
                            //    __getitem__(self, index: Literal[0], /) -> str
                            //    __getitem__(self, index: Literal[1], /) -> float | bytes
                            //    __getitem__(self, index: Literal[2], /) -> float | bytes | range
                            //    __getitem__(self, index: Literal[-1], /) -> range
                            //    __getitem__(self, index: Literal[-2], /) -> bytes
                            //    __getitem__(self, index: Literal[-3], /) -> float | str
                            //
                            TupleSpec::Variable(variable_length_tuple) => {
                                for (index, ty) in variable_length_tuple.prefix.iter().enumerate() {
                                    if let Ok(index) = i64::try_from(index) {
                                        element_type_to_indices.entry(*ty).or_default().push(index);
                                    }

                                    let one_based_index = index + 1;

                                    if let Ok(i) = i64::try_from(
                                        variable_length_tuple.suffix.len() + one_based_index,
                                    ) {
                                        let overload_return = UnionType::from_elements(
                                            db,
                                            std::iter::once(variable_length_tuple.variable).chain(
                                                variable_length_tuple
                                                    .prefix
                                                    .iter()
                                                    .rev()
                                                    .take(one_based_index)
                                                    .copied(),
                                            ),
                                        );
                                        element_type_to_indices
                                            .entry(overload_return)
                                            .or_default()
                                            .push(0 - i);
                                    }
                                }

                                for (index, ty) in
                                    variable_length_tuple.suffix.iter().rev().enumerate()
                                {
                                    if let Some(index) =
                                        index.checked_add(1).and_then(|i| i64::try_from(i).ok())
                                    {
                                        element_type_to_indices
                                            .entry(*ty)
                                            .or_default()
                                            .push(0 - index);
                                    }

                                    if let Ok(i) =
                                        i64::try_from(variable_length_tuple.prefix.len() + index)
                                    {
                                        let overload_return = UnionType::from_elements(
                                            db,
                                            std::iter::once(variable_length_tuple.variable).chain(
                                                variable_length_tuple
                                                    .suffix
                                                    .iter()
                                                    .take(index + 1)
                                                    .copied(),
                                            ),
                                        );
                                        element_type_to_indices
                                            .entry(overload_return)
                                            .or_default()
                                            .push(i);
                                    }
                                }
                            }
                        }

                        let all_elements_unioned =
                            UnionType::from_elements(db, tuple.all_elements());

                        let mut overload_signatures =
                            Vec::with_capacity(element_type_to_indices.len().saturating_add(2));

                        overload_signatures.extend(element_type_to_indices.into_iter().filter_map(
                            |(return_type, mut indices)| {
                                if return_type.is_equivalent_to(db, all_elements_unioned) {
                                    return None;
                                }

                                // Sorting isn't strictly required, but leads to nicer `reveal_type` output
                                indices.sort_unstable();

                                let index_annotation = UnionType::from_elements(
                                    db,
                                    indices.into_iter().map(Type::IntLiteral),
                                );

                                Some(synthesize_getitem_overload_signature(
                                    db,
                                    index_annotation,
                                    return_type,
                                ))
                            },
                        ));

                        // Fallback overloads: for `tuple[int, str]`, we will generate the following overloads:
                        //
                        //    __getitem__(self, index: int, /) -> int | str
                        //    __getitem__(self, index: slice[Any, Any, Any], /) -> tuple[int | str, ...]
                        //
                        // and for `tuple[str, *tuple[float, ...], bytes]`, we will generate the following overloads:
                        //
                        //    __getitem__(self, index: int, /) -> str | float | bytes
                        //    __getitem__(self, index: slice[Any, Any, Any], /) -> tuple[str | float | bytes, ...]
                        //
                        overload_signatures.push(synthesize_getitem_overload_signature(
                            db,
                            KnownClass::SupportsIndex.to_instance(db),
                            all_elements_unioned,
                        ));

                        overload_signatures.push(synthesize_getitem_overload_signature(
                            db,
                            KnownClass::Slice.to_instance(db),
                            Type::homogeneous_tuple(db, all_elements_unioned),
                        ));

                        let getitem_signature =
                            CallableSignature::from_overloads(overload_signatures);
                        let getitem_type = Type::Callable(CallableType::new(
                            db,
                            getitem_signature,
                            CallableTypeKind::FunctionLike,
                        ));
                        Member::definitely_declared(getitem_type)
                    })
                    .unwrap_or_else(fallback_member_lookup)
            }

            // ```py
            // class tuple:
            //     @overload
            //     def __new__(cls: type[tuple[()]], iterable: tuple[()] = ()) -> tuple[()]: ...
            //     @overload
            //     def __new__[T](cls: type[tuple[T, ...]], iterable: tuple[T, ...]) -> tuple[T, ...]: ...
            // ```
            "__new__" if class_literal.is_tuple(db) => {
                let mut iterable_parameter =
                    Parameter::positional_only(Some(Name::new_static("iterable")));

                let tuple = specialization.and_then(|spec| spec.tuple(db));

                match tuple {
                    Some(tuple) => {
                        // TODO: Once we support PEP 646 annotations for `*args` parameters, we can
                        // use the tuple itself as the argument type.
                        let tuple_len = tuple.len();

                        if tuple_len.minimum() == 0 && tuple_len.maximum().is_none() {
                            // If the tuple has no length restrictions,
                            // any iterable is allowed as long as the iterable has the correct element type.
                            let mut tuple_elements = tuple.all_elements();
                            iterable_parameter = iterable_parameter.with_annotated_type(
                                KnownClass::Iterable
                                    .to_specialized_instance(db, [*tuple_elements.next().unwrap()]),
                            );
                            assert_eq!(
                                tuple_elements.next(),
                                None,
                                "Tuple specialization should not have more than one element when it has no length restriction"
                            );
                        } else {
                            // But if the tuple is of a fixed length, or has a minimum length, we require a tuple rather
                            // than an iterable, as a tuple is the only kind of iterable for which we can
                            // specify a fixed length, or that the iterable must be at least a certain length.
                            iterable_parameter =
                                iterable_parameter.with_annotated_type(Type::instance(db, self));
                        }
                    }
                    None => {
                        // If the tuple isn't specialized at all, we allow any argument as long as it is iterable.
                        iterable_parameter = iterable_parameter
                            .with_annotated_type(KnownClass::Iterable.to_instance(db));
                    }
                }

                // We allow the `iterable` parameter to be omitted for:
                // - a zero-length tuple
                // - an unspecialized tuple
                // - a tuple with no minimum length
                if tuple.is_none_or(|tuple| tuple.len().minimum() == 0) {
                    iterable_parameter =
                        iterable_parameter.with_default_type(Type::empty_tuple(db));
                }

                let parameters = Parameters::new(
                    db,
                    [
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(SubclassOfType::from(db, self)),
                        iterable_parameter,
                    ],
                );

                let synthesized_dunder = Type::function_like_callable(
                    db,
                    Signature::new_generic(inherited_generic_context, parameters, None),
                );

                Member::definitely_declared(synthesized_dunder)
            }

            _ => fallback_member_lookup(),
        }
    }

    /// Look up an instance attribute (available in `__dict__`) of the given name.
    ///
    /// See [`Type::instance_member`] for more details.
    pub(super) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        let (class_literal, specialization) = self.class_literal(db);

        if class_literal.is_typed_dict(db) {
            return Place::Undefined.into();
        }

        class_literal
            .instance_member(db, specialization, name)
            .map_type(|ty| ty.apply_optional_specialization(db, specialization))
    }

    /// A helper function for `instance_member` that looks up the `name` attribute only on
    /// this class, not on its superclasses.
    fn own_instance_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal
            .own_instance_member(db, name)
            .map_type(|ty| ty.apply_optional_specialization(db, specialization))
    }

    /// Return a callable type (or union of callable types) that represents the callable
    /// constructor signature of this class.
    #[salsa::tracked(cycle_initial=into_callable_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(super) fn into_callable(self, db: &'db dyn Db) -> CallableTypes<'db> {
        // TODO: This mimics a lot of the logic in Type::try_call_from_constructor. Can we
        // consolidate the two? Can we invoke a class by upcasting the class into a Callable, and
        // then relying on the call binding machinery to Just Work™?

        let (class_literal, _) = self.class_literal(db);
        let class_generic_context = class_literal.generic_context(db);

        let self_ty = Type::from(self);
        let metaclass_dunder_call_function_symbol = self_ty
            .member_lookup_with_policy(
                db,
                "__call__".into(),
                MemberLookupPolicy::NO_INSTANCE_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            )
            .place;

        if let Place::Defined(Type::BoundMethod(metaclass_dunder_call_function), _, _) =
            metaclass_dunder_call_function_symbol
        {
            // TODO: this intentionally diverges from step 1 in
            // https://typing.python.org/en/latest/spec/constructors.html#converting-a-constructor-to-callable
            // by always respecting the signature of the metaclass `__call__`, rather than
            // using a heuristic which makes unwarranted assumptions to sometimes ignore it.
            return CallableTypes::one(metaclass_dunder_call_function.into_callable_type(db));
        }

        let dunder_new_function_symbol = self_ty.lookup_dunder_new(db);

        let dunder_new_signature = dunder_new_function_symbol
            .and_then(|place_and_quals| place_and_quals.ignore_possibly_undefined())
            .and_then(|ty| match ty {
                Type::FunctionLiteral(function) => Some(function.signature(db)),
                Type::Callable(callable) => Some(callable.signatures(db)),
                _ => None,
            });

        let dunder_new_function = if let Some(dunder_new_signature) = dunder_new_signature {
            // Step 3: If the return type of the `__new__` evaluates to a type that is not a subclass of this class,
            // then we should ignore the `__init__` and just return the `__new__` method.
            let returns_non_subclass = dunder_new_signature.overloads.iter().any(|signature| {
                signature.return_ty.is_some_and(|return_ty| {
                    !return_ty.is_assignable_to(
                        db,
                        self_ty
                            .to_instance(db)
                            .expect("ClassType should be instantiable"),
                    )
                })
            });

            let instance_ty = Type::instance(db, self);
            let dunder_new_bound_method = CallableType::new(
                db,
                dunder_new_signature.bind_self(db, Some(instance_ty)),
                CallableTypeKind::FunctionLike,
            );

            if returns_non_subclass {
                return CallableTypes::one(dunder_new_bound_method);
            }
            Some(dunder_new_bound_method)
        } else {
            None
        };

        let dunder_init_function_symbol = self_ty
            .member_lookup_with_policy(
                db,
                "__init__".into(),
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            )
            .place;

        let correct_return_type = self_ty.to_instance(db).unwrap_or_else(Type::unknown);

        // If the class defines an `__init__` method, then we synthesize a callable type with the
        // same parameters as the `__init__` method after it is bound, and with the return type of
        // the concrete type of `Self`.
        let synthesized_dunder_init_callable = if let Place::Defined(ty, _, _) =
            dunder_init_function_symbol
        {
            let signature = match ty {
                Type::FunctionLiteral(dunder_init_function) => {
                    Some(dunder_init_function.signature(db))
                }
                Type::Callable(callable) => Some(callable.signatures(db)),
                _ => None,
            };

            if let Some(signature) = signature {
                let synthesized_signature = |signature: &Signature<'db>| {
                    let self_annotation = signature
                        .parameters()
                        .get_positional(0)
                        .and_then(Parameter::annotated_type)
                        .filter(|ty| {
                            ty.as_typevar()
                                .is_none_or(|bound_typevar| !bound_typevar.typevar(db).is_self(db))
                        });
                    let return_type = self_annotation.unwrap_or(correct_return_type);
                    let instance_ty = self_annotation.unwrap_or_else(|| Type::instance(db, self));
                    let generic_context = GenericContext::merge_optional(
                        db,
                        class_generic_context,
                        signature.generic_context,
                    );
                    Signature::new_generic(
                        generic_context,
                        signature.parameters().clone(),
                        Some(return_type),
                    )
                    .with_definition(signature.definition())
                    .bind_self(db, Some(instance_ty))
                };

                let synthesized_dunder_init_signature = CallableSignature::from_overloads(
                    signature.overloads.iter().map(synthesized_signature),
                );

                Some(CallableType::new(
                    db,
                    synthesized_dunder_init_signature,
                    CallableTypeKind::FunctionLike,
                ))
            } else {
                None
            }
        } else {
            None
        };

        match (dunder_new_function, synthesized_dunder_init_callable) {
            (Some(dunder_new_function), Some(synthesized_dunder_init_callable)) => {
                CallableTypes::from_elements([
                    dunder_new_function,
                    synthesized_dunder_init_callable,
                ])
            }
            (Some(constructor), None) | (None, Some(constructor)) => {
                CallableTypes::one(constructor)
            }
            (None, None) => {
                // If no `__new__` or `__init__` method is found, then we fall back to looking for
                // an `object.__new__` method.
                let new_function_symbol = self_ty
                    .member_lookup_with_policy(
                        db,
                        "__new__".into(),
                        MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
                    )
                    .place;

                if let Place::Defined(Type::FunctionLiteral(mut new_function), _, _) =
                    new_function_symbol
                {
                    if let Some(class_generic_context) = class_generic_context {
                        new_function =
                            new_function.with_inherited_generic_context(db, class_generic_context);
                    }
                    CallableTypes::one(
                        new_function
                            .into_bound_method_type(db, correct_return_type)
                            .into_callable_type(db),
                    )
                } else {
                    // Fallback if no `object.__new__` is found.
                    CallableTypes::one(CallableType::single(
                        db,
                        Signature::new_generic(
                            class_generic_context,
                            Parameters::empty(),
                            Some(correct_return_type),
                        ),
                    ))
                }
            }
        }
    }

    pub(super) fn is_protocol(self, db: &'db dyn Db) -> bool {
        self.class_literal(db).0.is_protocol(db)
    }

    pub(super) fn header_span(self, db: &'db dyn Db) -> Span {
        self.class_literal(db).0.header_span(db)
    }
}

fn into_callable_cycle_initial<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    _self: ClassType<'db>,
) -> CallableTypes<'db> {
    CallableTypes::one(CallableType::bottom(db))
}

impl<'db> From<GenericAlias<'db>> for ClassType<'db> {
    fn from(generic: GenericAlias<'db>) -> ClassType<'db> {
        ClassType::Generic(generic)
    }
}

impl<'db> From<ClassType<'db>> for Type<'db> {
    fn from(class: ClassType<'db>) -> Type<'db> {
        match class {
            ClassType::NonGeneric(non_generic) => non_generic.into(),
            ClassType::Generic(generic) => generic.into(),
        }
    }
}

impl<'db> VarianceInferable<'db> for ClassType<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        match self {
            Self::NonGeneric(class) => class.variance_of(db, typevar),
            Self::Generic(generic) => generic.variance_of(db, typevar),
        }
    }
}

/// A filter that describes which methods are considered when looking for implicit attribute assignments
/// in [`ClassLiteral::implicit_attribute`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum MethodDecorator {
    None,
    ClassMethod,
    StaticMethod,
}

impl MethodDecorator {
    pub(crate) fn try_from_fn_type(db: &dyn Db, fn_type: FunctionType) -> Result<Self, ()> {
        match (fn_type.is_classmethod(db), fn_type.is_staticmethod(db)) {
            (true, true) => Err(()), // A method can't be static and class method at the same time.
            (true, false) => Ok(Self::ClassMethod),
            (false, true) => Ok(Self::StaticMethod),
            (false, false) => Ok(Self::None),
        }
    }

    pub(crate) const fn description(self) -> &'static str {
        match self {
            MethodDecorator::None => "an instance method",
            MethodDecorator::ClassMethod => "a classmethod",
            MethodDecorator::StaticMethod => "a staticmethod",
        }
    }
}

/// Kind-specific metadata for different types of fields
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) enum FieldKind<'db> {
    /// `NamedTuple` field metadata
    NamedTuple { default_ty: Option<Type<'db>> },
    /// dataclass field metadata
    Dataclass {
        /// The type of the default value for this field
        default_ty: Option<Type<'db>>,
        /// Whether or not this field is "init-only". If this is true, it only appears in the
        /// `__init__` signature, but is not accessible as a real field
        init_only: bool,
        /// Whether or not this field should appear in the signature of `__init__`.
        init: bool,
        /// Whether or not this field can only be passed as a keyword argument to `__init__`.
        kw_only: Option<bool>,
        /// The name for this field in the `__init__` signature, if specified.
        alias: Option<Box<str>>,
    },
    /// `TypedDict` field metadata
    TypedDict {
        /// Whether this field is required
        is_required: bool,
        /// Whether this field is marked read-only
        is_read_only: bool,
    },
}

/// Metadata regarding a dataclass field/attribute or a `TypedDict` "item" / key-value pair.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct Field<'db> {
    /// The declared type of the field
    pub(crate) declared_ty: Type<'db>,
    /// Kind-specific metadata for this field
    pub(crate) kind: FieldKind<'db>,
    /// The first declaration of this field.
    /// This field is used for backreferences in diagnostics.
    pub(crate) first_declaration: Option<Definition<'db>>,
}

impl Field<'_> {
    pub(crate) const fn is_required(&self) -> bool {
        match &self.kind {
            FieldKind::NamedTuple { default_ty } => default_ty.is_none(),
            // A dataclass field is NOT required if `default` (or `default_factory`) is set
            // or if `init` has been set to `False`.
            FieldKind::Dataclass {
                init, default_ty, ..
            } => default_ty.is_none() && *init,
            FieldKind::TypedDict { is_required, .. } => *is_required,
        }
    }

    pub(crate) const fn is_read_only(&self) -> bool {
        match &self.kind {
            FieldKind::TypedDict { is_read_only, .. } => *is_read_only,
            _ => false,
        }
    }
}

impl<'db> Field<'db> {
    /// Returns true if this field is a `dataclasses.KW_ONLY` sentinel.
    /// <https://docs.python.org/3/library/dataclasses.html#dataclasses.KW_ONLY>
    pub(crate) fn is_kw_only_sentinel(&self, db: &'db dyn Db) -> bool {
        self.declared_ty.is_instance_of(db, KnownClass::KwOnly)
    }
}

/// Representation of a class definition statement in the AST: either a non-generic class, or a
/// generic class that has not been specialized.
///
/// This does not in itself represent a type, but can be transformed into a [`ClassType`] that
/// does. (For generic classes, this requires specializing its generic context.)
///
/// # Ordering
/// Ordering is based on the class's id assigned by salsa and not on the class literal's values.
/// The id may change between runs, or when the class literal was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct ClassLiteral<'db> {
    /// Name of the class at definition
    #[returns(ref)]
    pub(crate) name: ast::name::Name,

    pub(crate) body_scope: ScopeId<'db>,

    pub(crate) known: Option<KnownClass>,

    /// If this class is deprecated, this holds the deprecation message.
    pub(crate) deprecated: Option<DeprecatedInstance<'db>>,

    pub(crate) type_check_only: bool,

    pub(crate) dataclass_params: Option<DataclassParams<'db>>,
    pub(crate) dataclass_transformer_params: Option<DataclassTransformerParams<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ClassLiteral<'_> {}

fn generic_context_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: ClassLiteral<'db>,
) -> Option<GenericContext<'db>> {
    None
}

#[salsa::tracked]
impl<'db> ClassLiteral<'db> {
    /// Return `true` if this class represents `known_class`
    pub(crate) fn is_known(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.known(db) == Some(known_class)
    }

    pub(crate) fn is_tuple(self, db: &'db dyn Db) -> bool {
        self.is_known(db, KnownClass::Tuple)
    }

    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        // Several typeshed definitions examine `sys.version_info`. To break cycles, we hard-code
        // the knowledge that this class is not generic.
        if self.is_known(db, KnownClass::VersionInfo) {
            return None;
        }

        // We've already verified that the class literal does not contain both a PEP-695 generic
        // scope and a `typing.Generic` base class.
        //
        // Note that if a class has an explicit legacy generic context (by inheriting from
        // `typing.Generic`), and also an implicit one (by inheriting from other generic classes,
        // specialized by typevars), the explicit one takes precedence.
        self.pep695_generic_context(db)
            .or_else(|| self.legacy_generic_context(db))
            .or_else(|| self.inherited_legacy_generic_context(db))
    }

    pub(crate) fn has_pep_695_type_params(self, db: &'db dyn Db) -> bool {
        self.pep695_generic_context(db).is_some()
    }

    #[salsa::tracked(cycle_initial=generic_context_cycle_initial,
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(crate) fn pep695_generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        let scope = self.body_scope(db);
        let file = scope.file(db);
        let parsed = parsed_module(db, file).load(db);
        let class_def_node = scope.node(db).expect_class().node(&parsed);
        class_def_node.type_params.as_ref().map(|type_params| {
            let index = semantic_index(db, scope.file(db));
            let definition = index.expect_single_definition(class_def_node);
            GenericContext::from_type_params(db, index, definition, type_params)
        })
    }

    pub(crate) fn legacy_generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.explicit_bases(db).iter().find_map(|base| match base {
            Type::KnownInstance(
                KnownInstanceType::SubscriptedGeneric(generic_context)
                | KnownInstanceType::SubscriptedProtocol(generic_context),
            ) => Some(*generic_context),
            _ => None,
        })
    }

    #[salsa::tracked(cycle_initial=generic_context_cycle_initial,
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(crate) fn inherited_legacy_generic_context(
        self,
        db: &'db dyn Db,
    ) -> Option<GenericContext<'db>> {
        GenericContext::from_base_classes(
            db,
            self.definition(db),
            self.explicit_bases(db)
                .iter()
                .copied()
                .filter(|ty| matches!(ty, Type::GenericAlias(_))),
        )
    }

    /// Returns all of the typevars that are referenced in this class's definition. This includes
    /// any typevars bound in its generic context, as well as any typevars mentioned in its base
    /// class list. (This is used to ensure that classes do not bind or reference typevars from
    /// enclosing generic contexts.)
    pub(crate) fn typevars_referenced_in_definition(
        self,
        db: &'db dyn Db,
    ) -> FxIndexSet<BoundTypeVarInstance<'db>> {
        #[derive(Default)]
        struct CollectTypeVars<'db> {
            typevars: RefCell<FxIndexSet<BoundTypeVarInstance<'db>>>,
            recursion_guard: TypeCollector<'db>,
        }

        impl<'db> TypeVisitor<'db> for CollectTypeVars<'db> {
            fn should_visit_lazy_type_attributes(&self) -> bool {
                false
            }

            fn visit_bound_type_var_type(
                &self,
                _db: &'db dyn Db,
                bound_typevar: BoundTypeVarInstance<'db>,
            ) {
                self.typevars.borrow_mut().insert(bound_typevar);
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
            }
        }

        let visitor = CollectTypeVars::default();
        if let Some(generic_context) = self.generic_context(db) {
            walk_generic_context(db, generic_context, &visitor);
        }
        for base in self.explicit_bases(db) {
            visitor.visit_type(db, *base);
        }
        visitor.typevars.into_inner()
    }

    /// Returns the generic context that should be inherited by any constructor methods of this class.
    fn inherited_generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.generic_context(db)
    }

    pub(super) fn file(self, db: &dyn Db) -> File {
        self.body_scope(db).file(db)
    }

    /// Return the original [`ast::StmtClassDef`] node associated with this class
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn node<'ast>(self, db: &'db dyn Db, module: &'ast ParsedModuleRef) -> &'ast ast::StmtClassDef {
        let scope = self.body_scope(db);
        scope.node(db).expect_class().node(module)
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let body_scope = self.body_scope(db);
        let index = semantic_index(db, body_scope.file(db));
        index.expect_single_definition(body_scope.node(db).expect_class())
    }

    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> ClassType<'db> {
        match self.generic_context(db) {
            None => ClassType::NonGeneric(self),
            Some(generic_context) => {
                let specialization = f(generic_context);

                ClassType::Generic(GenericAlias::new(db, self, specialization))
            }
        }
    }

    pub(crate) fn apply_optional_specialization(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            specialization
                .unwrap_or_else(|| generic_context.default_specialization(db, self.known(db)))
        })
    }

    pub(crate) fn top_materialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            generic_context
                .default_specialization(db, self.known(db))
                .materialize_impl(
                    db,
                    MaterializationKind::Top,
                    &ApplyTypeMappingVisitor::default(),
                )
        })
    }

    /// Returns the default specialization of this class. For non-generic classes, the class is
    /// returned unchanged. For a non-specialized generic class, we return a generic alias that
    /// applies the default specialization to the class's typevars.
    pub(crate) fn default_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            generic_context.default_specialization(db, self.known(db))
        })
    }

    /// Returns the unknown specialization of this class. For non-generic classes, the class is
    /// returned unchanged. For a non-specialized generic class, we return a generic alias that
    /// maps each of the class's typevars to `Unknown`.
    pub(crate) fn unknown_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            generic_context.unknown_specialization(db)
        })
    }

    /// Returns a specialization of this class where each typevar is mapped to itself.
    pub(crate) fn identity_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            generic_context.identity_specialization(db)
        })
    }

    /// Return an iterator over the inferred types of this class's *explicit* bases.
    ///
    /// Note that any class (except for `object`) that has no explicit
    /// bases will implicitly inherit from `object` at runtime. Nonetheless,
    /// this method does *not* include `object` in the bases it iterates over.
    ///
    /// ## Why is this a salsa query?
    ///
    /// This is a salsa query to short-circuit the invalidation
    /// when the class's AST node changes.
    ///
    /// Were this not a salsa query, then the calling query
    /// would depend on the class's AST and rerun for every change in that file.
    #[salsa::tracked(returns(deref), cycle_initial=explicit_bases_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(super) fn explicit_bases(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        tracing::trace!("ClassLiteral::explicit_bases_query: {}", self.name(db));

        let module = parsed_module(db, self.file(db)).load(db);
        let class_stmt = self.node(db, &module);
        let class_definition =
            semantic_index(db, self.file(db)).expect_single_definition(class_stmt);

        if self.is_known(db, KnownClass::VersionInfo) {
            let tuple_type = TupleType::new(db, &TupleSpec::version_info_spec(db))
                .expect("sys.version_info tuple spec should always be a valid tuple");

            Box::new([
                definition_expression_type(db, class_definition, &class_stmt.bases()[0]),
                Type::from(tuple_type.to_class_type(db)),
            ])
        } else {
            class_stmt
                .bases()
                .iter()
                .map(|base_node| definition_expression_type(db, class_definition, base_node))
                .collect()
        }
    }

    /// Return `Some()` if this class is known to be a [`DisjointBase`], or `None` if it is not.
    pub(super) fn as_disjoint_base(self, db: &'db dyn Db) -> Option<DisjointBase<'db>> {
        if self
            .known_function_decorators(db)
            .contains(&KnownFunction::DisjointBase)
        {
            Some(DisjointBase::due_to_decorator(self))
        } else if SlotsKind::from(db, self) == SlotsKind::NotEmpty {
            Some(DisjointBase::due_to_dunder_slots(self))
        } else {
            None
        }
    }

    /// Iterate over this class's explicit bases, filtering out any bases that are not class
    /// objects, and applying default specialization to any unspecialized generic class literals.
    fn fully_static_explicit_bases(self, db: &'db dyn Db) -> impl Iterator<Item = ClassType<'db>> {
        self.explicit_bases(db)
            .iter()
            .copied()
            .filter_map(|ty| ty.to_class_type(db))
    }

    /// Determine if this class is a protocol.
    ///
    /// This method relies on the accuracy of the [`KnownClass::is_protocol`] method,
    /// which hardcodes knowledge about certain special-cased classes. See the docs on
    /// that method for why we do this rather than relying on generalised logic for all
    /// classes, including the special-cased ones that are included in the [`KnownClass`]
    /// enum.
    pub(super) fn is_protocol(self, db: &'db dyn Db) -> bool {
        self.known(db)
            .map(KnownClass::is_protocol)
            .unwrap_or_else(|| {
                // Iterate through the last three bases of the class
                // searching for `Protocol` or `Protocol[]` in the bases list.
                //
                // If `Protocol` is present in the bases list of a valid protocol class, it must either:
                //
                // - be the last base
                // - OR be the last-but-one base (with the final base being `Generic[]` or `object`)
                // - OR be the last-but-two base (with the penultimate base being `Generic[]`
                //                                and the final base being `object`)
                self.explicit_bases(db).iter().rev().take(3).any(|base| {
                    matches!(
                        base,
                        Type::SpecialForm(SpecialFormType::Protocol)
                            | Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(_))
                    )
                })
            })
    }

    /// Determine if this is an abstract class.
    pub(super) fn is_abstract(self, db: &'db dyn Db) -> bool {
        self.metaclass(db)
            .as_class_literal()
            .is_some_and(|metaclass| metaclass.is_known(db, KnownClass::ABCMeta))
    }

    /// Return the types of the decorators on this class
    #[salsa::tracked(returns(deref), cycle_initial=decorators_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    fn decorators(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        tracing::trace!("ClassLiteral::decorators: {}", self.name(db));

        let module = parsed_module(db, self.file(db)).load(db);

        let class_stmt = self.node(db, &module);
        if class_stmt.decorator_list.is_empty() {
            return Box::new([]);
        }

        let class_definition =
            semantic_index(db, self.file(db)).expect_single_definition(class_stmt);

        class_stmt
            .decorator_list
            .iter()
            .map(|decorator_node| {
                definition_expression_type(db, class_definition, &decorator_node.expression)
            })
            .collect()
    }

    pub(super) fn known_function_decorators(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = KnownFunction> + 'db {
        self.decorators(db)
            .iter()
            .filter_map(|deco| deco.as_function_literal())
            .filter_map(|decorator| decorator.known(db))
    }

    /// Is this class final?
    pub(super) fn is_final(self, db: &'db dyn Db) -> bool {
        self.known_function_decorators(db)
            .contains(&KnownFunction::Final)
            || enum_metadata(db, self).is_some()
    }

    /// Attempt to resolve the [method resolution order] ("MRO") for this class.
    /// If the MRO is unresolvable, return an error indicating why the class's MRO
    /// cannot be accurately determined. The error returned contains a fallback MRO
    /// that will be used instead for the purposes of type inference.
    ///
    /// The MRO is the tuple of classes that can be retrieved as the `__mro__`
    /// attribute on a class at runtime.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    #[salsa::tracked(returns(as_ref), cycle_initial=try_mro_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(super) fn try_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, MroError<'db>> {
        tracing::trace!("ClassLiteral::try_mro: {}", self.name(db));
        Mro::of_class(db, self, specialization)
    }

    /// Iterate over the [method resolution order] ("MRO") of the class.
    ///
    /// If the MRO could not be accurately resolved, this method falls back to iterating
    /// over an MRO that has the class directly inheriting from `Unknown`. Use
    /// [`ClassLiteral::try_mro`] if you need to distinguish between the success and failure
    /// cases rather than simply iterating over the inferred resolution order for the class.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(super) fn iter_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> MroIterator<'db> {
        MroIterator::new(db, self, specialization)
    }

    /// Return `true` if `other` is present in this class's MRO.
    pub(super) fn is_subclass_of(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        other: ClassType<'db>,
    ) -> bool {
        // `is_subclass_of` is checking the subtype relation, in which gradual types do not
        // participate, so we should not return `True` if we find `Any/Unknown` in the MRO.
        self.iter_mro(db, specialization)
            .contains(&ClassBase::Class(other))
    }

    pub(super) fn when_subclass_of(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        other: ClassType<'db>,
    ) -> ConstraintSet<'db> {
        ConstraintSet::from(self.is_subclass_of(db, specialization, other))
    }

    /// Return `true` if this class constitutes a typed dict specification (inherits from
    /// `typing.TypedDict`, either directly or indirectly).
    #[salsa::tracked(cycle_initial=is_typed_dict_cycle_initial,
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(super) fn is_typed_dict(self, db: &'db dyn Db) -> bool {
        if let Some(known) = self.known(db) {
            return known.is_typed_dict_subclass();
        }

        self.iter_mro(db, None)
            .any(|base| matches!(base, ClassBase::TypedDict))
    }

    /// Compute `TypedDict` parameters dynamically based on MRO detection and AST parsing.
    fn typed_dict_params(self, db: &'db dyn Db) -> Option<TypedDictParams> {
        if !self.is_typed_dict(db) {
            return None;
        }

        let module = parsed_module(db, self.file(db)).load(db);
        let class_stmt = self.node(db, &module);
        Some(typed_dict_params_from_class_def(class_stmt))
    }

    /// Return the explicit `metaclass` of this class, if one is defined.
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn explicit_metaclass(self, db: &'db dyn Db, module: &ParsedModuleRef) -> Option<Type<'db>> {
        let class_stmt = self.node(db, module);
        let metaclass_node = &class_stmt
            .arguments
            .as_ref()?
            .find_keyword("metaclass")?
            .value;

        let class_definition = self.definition(db);

        Some(definition_expression_type(
            db,
            class_definition,
            metaclass_node,
        ))
    }

    /// Return the metaclass of this class, or `type[Unknown]` if the metaclass cannot be inferred.
    pub(super) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        self.try_metaclass(db)
            .map(|(ty, _)| ty)
            .unwrap_or_else(|_| SubclassOfType::subclass_of_unknown())
    }

    /// Return a type representing "the set of all instances of the metaclass of this class".
    pub(super) fn metaclass_instance_type(self, db: &'db dyn Db) -> Type<'db> {
        self
            .metaclass(db)
            .to_instance(db)
            .expect("`Type::to_instance()` should always return `Some()` when called on the type of a metaclass")
    }

    /// Return the metaclass of this class, or an error if the metaclass cannot be inferred.
    #[salsa::tracked(cycle_initial=try_metaclass_cycle_initial,
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(super) fn try_metaclass(
        self,
        db: &'db dyn Db,
    ) -> Result<(Type<'db>, Option<DataclassTransformerParams<'db>>), MetaclassError<'db>> {
        tracing::trace!("ClassLiteral::try_metaclass: {}", self.name(db));

        // Identify the class's own metaclass (or take the first base class's metaclass).
        let mut base_classes = self.fully_static_explicit_bases(db).peekable();

        if base_classes.peek().is_some() && self.inheritance_cycle(db).is_some() {
            // We emit diagnostics for cyclic class definitions elsewhere.
            // Avoid attempting to infer the metaclass if the class is cyclically defined.
            return Ok((SubclassOfType::subclass_of_unknown(), None));
        }

        if self.try_mro(db, None).is_err_and(MroError::is_cycle) {
            return Ok((SubclassOfType::subclass_of_unknown(), None));
        }

        let module = parsed_module(db, self.file(db)).load(db);

        let explicit_metaclass = self.explicit_metaclass(db, &module);
        let (metaclass, class_metaclass_was_from) = if let Some(metaclass) = explicit_metaclass {
            (metaclass, self)
        } else if let Some(base_class) = base_classes.next() {
            let (base_class_literal, _) = base_class.class_literal(db);
            (base_class.metaclass(db), base_class_literal)
        } else {
            (KnownClass::Type.to_class_literal(db), self)
        };

        let mut candidate = if let Some(metaclass_ty) = metaclass.to_class_type(db) {
            MetaclassCandidate {
                metaclass: metaclass_ty,
                explicit_metaclass_of: class_metaclass_was_from,
            }
        } else {
            let name = Type::string_literal(db, self.name(db));
            let bases = Type::heterogeneous_tuple(db, self.explicit_bases(db));
            let namespace = KnownClass::Dict
                .to_specialized_instance(db, [KnownClass::Str.to_instance(db), Type::any()]);

            // TODO: Other keyword arguments?
            let arguments = CallArguments::positional([name, bases, namespace]);

            let return_ty_result = match metaclass.try_call(db, &arguments) {
                Ok(bindings) => Ok(bindings.return_type(db)),

                Err(CallError(CallErrorKind::NotCallable, bindings)) => Err(MetaclassError {
                    kind: MetaclassErrorKind::NotCallable(bindings.callable_type()),
                }),

                // TODO we should also check for binding errors that would indicate the metaclass
                // does not accept the right arguments
                Err(CallError(CallErrorKind::BindingError, bindings)) => {
                    Ok(bindings.return_type(db))
                }

                Err(CallError(CallErrorKind::PossiblyNotCallable, _)) => Err(MetaclassError {
                    kind: MetaclassErrorKind::PartlyNotCallable(metaclass),
                }),
            };

            return return_ty_result.map(|ty| (ty.to_meta_type(db), None));
        };

        // Reconcile all base classes' metaclasses with the candidate metaclass.
        //
        // See:
        // - https://docs.python.org/3/reference/datamodel.html#determining-the-appropriate-metaclass
        // - https://github.com/python/cpython/blob/83ba8c2bba834c0b92de669cac16fcda17485e0e/Objects/typeobject.c#L3629-L3663
        for base_class in base_classes {
            let metaclass = base_class.metaclass(db);
            let Some(metaclass) = metaclass.to_class_type(db) else {
                continue;
            };
            if metaclass.is_subclass_of(db, candidate.metaclass) {
                let (base_class_literal, _) = base_class.class_literal(db);
                candidate = MetaclassCandidate {
                    metaclass,
                    explicit_metaclass_of: base_class_literal,
                };
                continue;
            }
            if candidate.metaclass.is_subclass_of(db, metaclass) {
                continue;
            }
            let (base_class_literal, _) = base_class.class_literal(db);
            return Err(MetaclassError {
                kind: MetaclassErrorKind::Conflict {
                    candidate1: candidate,
                    candidate2: MetaclassCandidate {
                        metaclass,
                        explicit_metaclass_of: base_class_literal,
                    },
                    candidate1_is_base_class: explicit_metaclass.is_none(),
                },
            });
        }

        let (metaclass_literal, _) = candidate.metaclass.class_literal(db);
        Ok((
            candidate.metaclass.into(),
            metaclass_literal.dataclass_transformer_params(db),
        ))
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member on the class itself or any of its proper superclasses.
    ///
    /// TODO: Should this be made private...?
    pub(super) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        fn into_function_like_callable<'d>(db: &'d dyn Db, ty: Type<'d>) -> Type<'d> {
            match ty {
                Type::Callable(callable_ty) => Type::Callable(CallableType::new(
                    db,
                    callable_ty.signatures(db),
                    CallableTypeKind::FunctionLike,
                )),
                Type::Union(union) => {
                    union.map(db, |element| into_function_like_callable(db, *element))
                }
                Type::Intersection(intersection) => intersection
                    .map_positive(db, |element| into_function_like_callable(db, *element)),
                _ => ty,
            }
        }

        let mut member = self.class_member_inner(db, None, name, policy);

        // We generally treat dunder attributes with `Callable` types as function-like callables.
        // See `callables_as_descriptors.md` for more details.
        if name.starts_with("__") && name.ends_with("__") {
            member = member.map_type(|ty| into_function_like_callable(db, ty));
        }

        member
    }

    fn class_member_inner(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        self.class_member_from_mro(db, name, policy, self.iter_mro(db, specialization))
    }

    pub(super) fn class_member_from_mro(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
        mro_iter: impl Iterator<Item = ClassBase<'db>>,
    ) -> PlaceAndQualifiers<'db> {
        // If we encounter a dynamic type in this class's MRO, we'll save that dynamic type
        // in this variable. After we've traversed the MRO, we'll either:
        // (1) Use that dynamic type as the type for this attribute,
        //     if no other classes in the MRO define the attribute; or,
        // (2) Intersect that dynamic type with the type of the attribute
        //     from the non-dynamic members of the class's MRO.
        let mut dynamic_type_to_intersect_with: Option<Type<'db>> = None;

        let mut lookup_result: LookupResult<'db> =
            Err(LookupError::Undefined(TypeQualifiers::empty()));

        for superclass in mro_iter {
            match superclass {
                ClassBase::Generic | ClassBase::Protocol => {
                    // Skip over these very special class bases that aren't really classes.
                }
                ClassBase::Dynamic(_) => {
                    // Note: calling `Type::from(superclass).member()` would be incorrect here.
                    // What we'd really want is a `Type::Any.own_class_member()` method,
                    // but adding such a method wouldn't make much sense -- it would always return `Any`!
                    dynamic_type_to_intersect_with.get_or_insert(Type::from(superclass));
                }
                ClassBase::Class(class) => {
                    let known = class.known(db);

                    if known == Some(KnownClass::Object)
                        // Only exclude `object` members if this is not an `object` class itself
                        && (policy.mro_no_object_fallback() && !self.is_known(db, KnownClass::Object))
                    {
                        continue;
                    }

                    if known == Some(KnownClass::Type) && policy.meta_class_no_type_fallback() {
                        continue;
                    }

                    if matches!(known, Some(KnownClass::Int | KnownClass::Str))
                        && policy.mro_no_int_or_str_fallback()
                    {
                        continue;
                    }

                    lookup_result = lookup_result.or_else(|lookup_error| {
                        lookup_error.or_fall_back_to(
                            db,
                            class
                                .own_class_member(db, self.inherited_generic_context(db), name)
                                .inner,
                        )
                    });
                }
                ClassBase::TypedDict => {
                    return KnownClass::TypedDictFallback
                        .to_class_literal(db)
                        .find_name_in_mro_with_policy(db, name, policy)
                        .expect("Will return Some() when called on class literal")
                        .map_type(|ty| {
                            ty.apply_type_mapping(
                                db,
                                &TypeMapping::ReplaceSelf {
                                    new_upper_bound: determine_upper_bound(
                                        db,
                                        self,
                                        None,
                                        ClassBase::is_typed_dict,
                                    ),
                                },
                                TypeContext::default(),
                            )
                        });
                }
            }
            if lookup_result.is_ok() {
                break;
            }
        }

        match (
            PlaceAndQualifiers::from(lookup_result),
            dynamic_type_to_intersect_with,
        ) {
            (symbol_and_qualifiers, None) => symbol_and_qualifiers,

            (
                PlaceAndQualifiers {
                    place: Place::Defined(ty, _, _),
                    qualifiers,
                },
                Some(dynamic_type),
            ) => Place::bound(
                IntersectionBuilder::new(db)
                    .add_positive(ty)
                    .add_positive(dynamic_type)
                    .build(),
            )
            .with_qualifiers(qualifiers),

            (
                PlaceAndQualifiers {
                    place: Place::Undefined,
                    qualifiers,
                },
                Some(dynamic_type),
            ) => Place::bound(dynamic_type).with_qualifiers(qualifiers),
        }
    }

    /// Returns the inferred type of the class member named `name`. Only bound members
    /// or those marked as `ClassVars` are considered.
    ///
    /// Returns [`Place::Undefined`] if `name` cannot be found in this class's scope
    /// directly. Use [`ClassLiteral::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(super) fn own_class_member(
        self,
        db: &'db dyn Db,
        inherited_generic_context: Option<GenericContext<'db>>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Member<'db> {
        if self.dataclass_params(db).is_some() {
            if name == "__dataclass_fields__" {
                // Make this class look like a subclass of the `DataClassInstance` protocol
                return Member {
                    inner: Place::declared(KnownClass::Dict.to_specialized_instance(
                        db,
                        [
                            KnownClass::Str.to_instance(db),
                            KnownClass::Field.to_specialized_instance(db, [Type::any()]),
                        ],
                    ))
                    .with_qualifiers(TypeQualifiers::CLASS_VAR),
                };
            } else if name == "__dataclass_params__" {
                // There is no typeshed class for this. For now, we model it as `Any`.
                return Member {
                    inner: Place::declared(Type::any()).with_qualifiers(TypeQualifiers::CLASS_VAR),
                };
            }
        }

        if CodeGeneratorKind::NamedTuple.matches(db, self, specialization) {
            if let Some(field) = self
                .own_fields(db, specialization, CodeGeneratorKind::NamedTuple)
                .get(name)
            {
                let property_getter_signature = Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_only(Some(Name::new_static("self")))],
                    ),
                    Some(field.declared_ty),
                );
                let property_getter = Type::single_callable(db, property_getter_signature);
                let property = PropertyInstanceType::new(db, Some(property_getter), None);
                return Member::definitely_declared(Type::PropertyInstance(property));
            }
        }

        let body_scope = self.body_scope(db);
        let member = class_member(db, body_scope, name).map_type(|ty| {
            // The `__new__` and `__init__` members of a non-specialized generic class are handled
            // specially: they inherit the generic context of their class. That lets us treat them
            // as generic functions when constructing the class, and infer the specialization of
            // the class from the arguments that are passed in.
            //
            // We might decide to handle other class methods the same way, having them inherit the
            // class's generic context, and performing type inference on calls to them to determine
            // the specialization of the class. If we do that, we would update this to also apply
            // to any method with a `@classmethod` decorator. (`__init__` would remain a special
            // case, since it's an _instance_ method where we don't yet know the generic class's
            // specialization.)
            match (inherited_generic_context, ty, specialization, name) {
                (
                    Some(generic_context),
                    Type::FunctionLiteral(function),
                    Some(_),
                    "__new__" | "__init__",
                ) => Type::FunctionLiteral(
                    function.with_inherited_generic_context(db, generic_context),
                ),
                _ => ty,
            }
        });

        if member.is_undefined() {
            if let Some(synthesized_member) =
                self.own_synthesized_member(db, specialization, inherited_generic_context, name)
            {
                return Member::definitely_declared(synthesized_member);
            }
            // The symbol was not found in the class scope. It might still be implicitly defined in `@classmethod`s.
            return Self::implicit_attribute(db, body_scope, name, MethodDecorator::ClassMethod);
        }
        member
    }

    /// Returns the type of a synthesized dataclass member like `__init__` or `__lt__`, or
    /// a synthesized `__new__` method for a `NamedTuple`.
    pub(super) fn own_synthesized_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        inherited_generic_context: Option<GenericContext<'db>>,
        name: &str,
    ) -> Option<Type<'db>> {
        let dataclass_params = self.dataclass_params(db);

        let field_policy = CodeGeneratorKind::from_class(db, self, specialization)?;

        let mut transformer_params =
            if let CodeGeneratorKind::DataclassLike(Some(transformer_params)) = field_policy {
                Some(DataclassParams::from_transformer_params(
                    db,
                    transformer_params,
                ))
            } else {
                None
            };

        // Dataclass transformer flags can be overwritten using class arguments.
        if let Some(transformer_params) = transformer_params.as_mut() {
            if let Some(class_def) = self.definition(db).kind(db).as_class() {
                let module = parsed_module(db, self.file(db)).load(db);

                if let Some(arguments) = &class_def.node(&module).arguments {
                    let mut flags = transformer_params.flags(db);

                    for keyword in &arguments.keywords {
                        if let Some(arg_name) = &keyword.arg {
                            if let Some(is_set) =
                                keyword.value.as_boolean_literal_expr().map(|b| b.value)
                            {
                                for (flag_name, flag) in DATACLASS_FLAGS {
                                    if arg_name.as_str() == *flag_name {
                                        flags.set(*flag, is_set);
                                    }
                                }
                            }
                        }
                    }

                    *transformer_params =
                        DataclassParams::new(db, flags, transformer_params.field_specifiers(db));
                }
            }
        }

        let has_dataclass_param = |param| {
            dataclass_params.is_some_and(|params| params.flags(db).contains(param))
                || transformer_params.is_some_and(|params| params.flags(db).contains(param))
        };

        let instance_ty =
            Type::instance(db, self.apply_optional_specialization(db, specialization));

        let signature_from_fields = |mut parameters: Vec<_>, return_ty: Option<Type<'db>>| {
            for (field_name, field) in self.fields(db, specialization, field_policy) {
                let (init, mut default_ty, kw_only, alias) = match &field.kind {
                    FieldKind::NamedTuple { default_ty } => (true, *default_ty, None, None),
                    FieldKind::Dataclass {
                        init,
                        default_ty,
                        kw_only,
                        alias,
                        ..
                    } => (*init, *default_ty, *kw_only, alias.as_ref()),
                    FieldKind::TypedDict { .. } => continue,
                };
                let mut field_ty = field.declared_ty;

                if name == "__init__" && !init {
                    // Skip fields with `init=False`
                    continue;
                }

                if field.is_kw_only_sentinel(db) {
                    // Attributes annotated with `dataclass.KW_ONLY` are not present in the synthesized
                    // `__init__` method; they are used to indicate that the following parameters are
                    // keyword-only.
                    continue;
                }

                let dunder_set = field_ty.class_member(db, "__set__".into());
                if let Place::Defined(dunder_set, _, Definedness::AlwaysDefined) = dunder_set.place
                {
                    // The descriptor handling below is guarded by this not-dynamic check, because
                    // dynamic types like `Any` are valid (data) descriptors: since they have all
                    // possible attributes, they also have a (callable) `__set__` method. The
                    // problem is that we can't determine the type of the value parameter this way.
                    // Instead, we want to use the dynamic type itself in this case, so we skip the
                    // special descriptor handling.
                    if !dunder_set.is_dynamic() {
                        // This type of this attribute is a data descriptor. Instead of overwriting the
                        // descriptor attribute, data-classes will (implicitly) call the `__set__` method
                        // of the descriptor. This means that the synthesized `__init__` parameter for
                        // this attribute is determined by possible `value` parameter types with which
                        // the `__set__` method can be called. We build a union of all possible options
                        // to account for possible overloads.
                        let mut value_types = UnionBuilder::new(db);
                        for binding in &dunder_set.bindings(db) {
                            for overload in binding {
                                if let Some(value_param) =
                                    overload.signature.parameters().get_positional(2)
                                {
                                    value_types = value_types.add(
                                        value_param.annotated_type().unwrap_or_else(Type::unknown),
                                    );
                                } else if overload.signature.parameters().is_gradual() {
                                    value_types = value_types.add(Type::unknown());
                                }
                            }
                        }
                        field_ty = value_types.build();

                        // The default value of the attribute is *not* determined by the right hand side
                        // of the class-body assignment. Instead, the runtime invokes `__get__` on the
                        // descriptor, as if it had been called on the class itself, i.e. it passes `None`
                        // for the `instance` argument.

                        if let Some(ref mut default_ty) = default_ty {
                            *default_ty = default_ty
                                .try_call_dunder_get(db, Type::none(db), Type::ClassLiteral(self))
                                .map(|(return_ty, _)| return_ty)
                                .unwrap_or_else(Type::unknown);
                        }
                    }
                }

                let is_kw_only = name == "__replace__"
                    || kw_only.unwrap_or(has_dataclass_param(DataclassFlags::KW_ONLY));

                // Use the alias name if provided, otherwise use the field name
                let parameter_name =
                    Name::new(alias.map(|alias| &**alias).unwrap_or(&**field_name));

                let mut parameter = if is_kw_only {
                    Parameter::keyword_only(parameter_name)
                } else {
                    Parameter::positional_or_keyword(parameter_name)
                }
                .with_annotated_type(field_ty);

                if name == "__replace__" {
                    // When replacing, we know there is a default value for the field
                    // (the value that is currently assigned to the field)
                    // assume this to be the declared type of the field
                    parameter = parameter.with_default_type(field_ty);
                } else if let Some(default_ty) = default_ty {
                    parameter = parameter.with_default_type(default_ty);
                }

                parameters.push(parameter);
            }

            // In the event that we have a mix of keyword-only and positional parameters, we need to sort them
            // so that the keyword-only parameters appear after positional parameters.
            parameters.sort_by_key(Parameter::is_keyword_only);

            let signature = match name {
                "__new__" | "__init__" => Signature::new_generic(
                    inherited_generic_context.or_else(|| self.inherited_generic_context(db)),
                    Parameters::new(db, parameters),
                    return_ty,
                ),
                _ => Signature::new(Parameters::new(db, parameters), return_ty),
            };
            Some(Type::function_like_callable(db, signature))
        };

        match (field_policy, name) {
            (CodeGeneratorKind::DataclassLike(_), "__init__") => {
                if !has_dataclass_param(DataclassFlags::INIT) {
                    return None;
                }

                let self_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                    // TODO: could be `Self`.
                    .with_annotated_type(instance_ty);
                signature_from_fields(vec![self_parameter], Some(Type::none(db)))
            }
            (CodeGeneratorKind::NamedTuple, "__new__") => {
                let cls_parameter = Parameter::positional_or_keyword(Name::new_static("cls"))
                    .with_annotated_type(KnownClass::Type.to_instance(db));
                signature_from_fields(vec![cls_parameter], Some(Type::none(db)))
            }
            (CodeGeneratorKind::DataclassLike(_), "__lt__" | "__le__" | "__gt__" | "__ge__") => {
                if !has_dataclass_param(DataclassFlags::ORDER) {
                    return None;
                }

                let signature = Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_or_keyword(Name::new_static("self"))
                                // TODO: could be `Self`.
                                .with_annotated_type(instance_ty),
                            Parameter::positional_or_keyword(Name::new_static("other"))
                                // TODO: could be `Self`.
                                .with_annotated_type(instance_ty),
                        ],
                    ),
                    Some(KnownClass::Bool.to_instance(db)),
                );

                Some(Type::function_like_callable(db, signature))
            }
            (CodeGeneratorKind::DataclassLike(_), "__hash__") => {
                let unsafe_hash = has_dataclass_param(DataclassFlags::UNSAFE_HASH);
                let frozen = has_dataclass_param(DataclassFlags::FROZEN);
                let eq = has_dataclass_param(DataclassFlags::EQ);

                if unsafe_hash || (frozen && eq) {
                    let signature = Signature::new(
                        Parameters::new(
                            db,
                            [Parameter::positional_or_keyword(Name::new_static("self"))
                                .with_annotated_type(instance_ty)],
                        ),
                        Some(KnownClass::Int.to_instance(db)),
                    );

                    Some(Type::function_like_callable(db, signature))
                } else if eq && !frozen {
                    Some(Type::none(db))
                } else {
                    // No `__hash__` is generated, fall back to `object.__hash__`
                    None
                }
            }
            (CodeGeneratorKind::DataclassLike(_), "__match_args__")
                if Program::get(db).python_version(db) >= PythonVersion::PY310 =>
            {
                if !has_dataclass_param(DataclassFlags::MATCH_ARGS) {
                    return None;
                }

                let kw_only_default = has_dataclass_param(DataclassFlags::KW_ONLY);

                let fields = self.fields(db, specialization, field_policy);
                let match_args = fields
                    .iter()
                    .filter(|(_, field)| {
                        if let FieldKind::Dataclass { init, kw_only, .. } = &field.kind {
                            *init && !kw_only.unwrap_or(kw_only_default)
                        } else {
                            false
                        }
                    })
                    .map(|(name, _)| Type::string_literal(db, name));
                Some(Type::heterogeneous_tuple(db, match_args))
            }
            (CodeGeneratorKind::DataclassLike(_), "__weakref__")
                if Program::get(db).python_version(db) >= PythonVersion::PY311 =>
            {
                if !has_dataclass_param(DataclassFlags::WEAKREF_SLOT)
                    || !has_dataclass_param(DataclassFlags::SLOTS)
                {
                    return None;
                }

                // This could probably be `weakref | None`, but it does not seem important enough to
                // model it precisely.
                Some(UnionType::from_elements(db, [Type::any(), Type::none(db)]))
            }
            (CodeGeneratorKind::NamedTuple, name) if name != "__init__" => {
                KnownClass::NamedTupleFallback
                    .to_class_literal(db)
                    .as_class_literal()?
                    .own_class_member(db, self.inherited_generic_context(db), None, name)
                    .ignore_possibly_undefined()
                    .map(|ty| {
                        ty.apply_type_mapping(
                            db,
                            &TypeMapping::ReplaceSelf {
                                new_upper_bound: determine_upper_bound(
                                    db,
                                    self,
                                    specialization,
                                    |base| {
                                        base.into_class()
                                            .is_some_and(|c| c.is_known(db, KnownClass::Tuple))
                                    },
                                ),
                            },
                            TypeContext::default(),
                        )
                    })
            }
            (CodeGeneratorKind::DataclassLike(_), "__replace__")
                if Program::get(db).python_version(db) >= PythonVersion::PY313 =>
            {
                let self_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                    .with_annotated_type(instance_ty);

                signature_from_fields(vec![self_parameter], Some(instance_ty))
            }
            (CodeGeneratorKind::DataclassLike(_), "__setattr__") => {
                if has_dataclass_param(DataclassFlags::FROZEN) {
                    let signature = Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_or_keyword(Name::new_static("self"))
                                    .with_annotated_type(instance_ty),
                                Parameter::positional_or_keyword(Name::new_static("name")),
                                Parameter::positional_or_keyword(Name::new_static("value")),
                            ],
                        ),
                        Some(Type::Never),
                    );

                    return Some(Type::function_like_callable(db, signature));
                }
                None
            }
            (CodeGeneratorKind::DataclassLike(_), "__slots__")
                if Program::get(db).python_version(db) >= PythonVersion::PY310 =>
            {
                has_dataclass_param(DataclassFlags::SLOTS).then(|| {
                    let fields = self.fields(db, specialization, field_policy);
                    let slots = fields.keys().map(|name| Type::string_literal(db, name));
                    Type::heterogeneous_tuple(db, slots)
                })
            }
            (CodeGeneratorKind::TypedDict, "__setitem__") => {
                let fields = self.fields(db, specialization, field_policy);

                // Add (key type, value type) overloads for all TypedDict items ("fields") that are not read-only:

                let mut writeable_fields = fields
                    .iter()
                    .filter(|(_, field)| !field.is_read_only())
                    .peekable();

                if writeable_fields.peek().is_none() {
                    // If there are no writeable fields, synthesize a `__setitem__` that takes
                    // a `key` of type `Never` to signal that no keys are accepted. This leads
                    // to slightly more user-friendly error messages compared to returning an
                    // empty overload set.
                    return Some(Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(Type::Never),
                                    Parameter::positional_only(Some(Name::new_static("value")))
                                        .with_annotated_type(Type::any()),
                                ],
                            ),
                            Some(Type::none(db)),
                        )),
                        CallableTypeKind::FunctionLike,
                    )));
                }

                let overloads = writeable_fields.map(|(name, field)| {
                    let key_type = Type::StringLiteral(StringLiteralType::new(db, name.as_str()));

                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_only(Some(Name::new_static("self")))
                                    .with_annotated_type(instance_ty),
                                Parameter::positional_only(Some(Name::new_static("key")))
                                    .with_annotated_type(key_type),
                                Parameter::positional_only(Some(Name::new_static("value")))
                                    .with_annotated_type(field.declared_ty),
                            ],
                        ),
                        Some(Type::none(db)),
                    )
                });

                Some(Type::Callable(CallableType::new(
                    db,
                    CallableSignature::from_overloads(overloads),
                    CallableTypeKind::FunctionLike,
                )))
            }
            (CodeGeneratorKind::TypedDict, "__getitem__") => {
                let fields = self.fields(db, specialization, field_policy);

                // Add (key -> value type) overloads for all TypedDict items ("fields"):
                let overloads = fields.iter().map(|(name, field)| {
                    let key_type = Type::StringLiteral(StringLiteralType::new(db, name.as_str()));

                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_only(Some(Name::new_static("self")))
                                    .with_annotated_type(instance_ty),
                                Parameter::positional_only(Some(Name::new_static("key")))
                                    .with_annotated_type(key_type),
                            ],
                        ),
                        Some(field.declared_ty),
                    )
                });

                Some(Type::Callable(CallableType::new(
                    db,
                    CallableSignature::from_overloads(overloads),
                    CallableTypeKind::FunctionLike,
                )))
            }
            (CodeGeneratorKind::TypedDict, "get") => {
                let overloads = self
                    .fields(db, specialization, field_policy)
                    .iter()
                    .flat_map(|(name, field)| {
                        let key_type =
                            Type::StringLiteral(StringLiteralType::new(db, name.as_str()));

                        // For a required key, `.get()` always returns the value type. For a non-required key,
                        // `.get()` returns the union of the value type and the type of the default argument
                        // (which defaults to `None`).

                        // TODO: For now, we use two overloads here. They can be merged into a single function
                        // once the generics solver takes default arguments into account.

                        let get_sig = Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(key_type),
                                ],
                            ),
                            Some(if field.is_required() {
                                field.declared_ty
                            } else {
                                UnionType::from_elements(db, [field.declared_ty, Type::none(db)])
                            }),
                        );

                        let t_default =
                            BoundTypeVarInstance::synthetic(db, "T", TypeVarVariance::Covariant);

                        let get_with_default_sig = Signature::new_generic(
                            Some(GenericContext::from_typevar_instances(db, [t_default])),
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(key_type),
                                    Parameter::positional_only(Some(Name::new_static("default")))
                                        .with_annotated_type(Type::TypeVar(t_default)),
                                ],
                            ),
                            Some(if field.is_required() {
                                field.declared_ty
                            } else {
                                UnionType::from_elements(
                                    db,
                                    [field.declared_ty, Type::TypeVar(t_default)],
                                )
                            }),
                        );

                        [get_sig, get_with_default_sig]
                    })
                    // Fallback overloads for unknown keys
                    .chain(std::iter::once({
                        Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(KnownClass::Str.to_instance(db)),
                                ],
                            ),
                            Some(UnionType::from_elements(
                                db,
                                [Type::unknown(), Type::none(db)],
                            )),
                        )
                    }))
                    .chain(std::iter::once({
                        let t_default =
                            BoundTypeVarInstance::synthetic(db, "T", TypeVarVariance::Covariant);

                        Signature::new_generic(
                            Some(GenericContext::from_typevar_instances(db, [t_default])),
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(KnownClass::Str.to_instance(db)),
                                    Parameter::positional_only(Some(Name::new_static("default")))
                                        .with_annotated_type(Type::TypeVar(t_default)),
                                ],
                            ),
                            Some(UnionType::from_elements(
                                db,
                                [Type::unknown(), Type::TypeVar(t_default)],
                            )),
                        )
                    }));

                Some(Type::Callable(CallableType::new(
                    db,
                    CallableSignature::from_overloads(overloads),
                    CallableTypeKind::FunctionLike,
                )))
            }
            (CodeGeneratorKind::TypedDict, "pop") => {
                let fields = self.fields(db, specialization, field_policy);
                let overloads = fields
                    .iter()
                    .filter(|(_, field)| {
                        // Only synthesize `pop` for fields that are not required.
                        !field.is_required()
                    })
                    .flat_map(|(name, field)| {
                        let key_type =
                            Type::StringLiteral(StringLiteralType::new(db, name.as_str()));

                        // TODO: Similar to above: consider merging these two overloads into one

                        // `.pop()` without default
                        let pop_sig = Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(key_type),
                                ],
                            ),
                            Some(field.declared_ty),
                        );

                        // `.pop()` with a default value
                        let t_default =
                            BoundTypeVarInstance::synthetic(db, "T", TypeVarVariance::Covariant);

                        let pop_with_default_sig = Signature::new_generic(
                            Some(GenericContext::from_typevar_instances(db, [t_default])),
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("self")))
                                        .with_annotated_type(instance_ty),
                                    Parameter::positional_only(Some(Name::new_static("key")))
                                        .with_annotated_type(key_type),
                                    Parameter::positional_only(Some(Name::new_static("default")))
                                        .with_annotated_type(Type::TypeVar(t_default)),
                                ],
                            ),
                            Some(UnionType::from_elements(
                                db,
                                [field.declared_ty, Type::TypeVar(t_default)],
                            )),
                        );

                        [pop_sig, pop_with_default_sig]
                    });

                Some(Type::Callable(CallableType::new(
                    db,
                    CallableSignature::from_overloads(overloads),
                    CallableTypeKind::FunctionLike,
                )))
            }
            (CodeGeneratorKind::TypedDict, "setdefault") => {
                let fields = self.fields(db, specialization, field_policy);
                let overloads = fields.iter().map(|(name, field)| {
                    let key_type = Type::StringLiteral(StringLiteralType::new(db, name.as_str()));

                    // `setdefault` always returns the field type
                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_only(Some(Name::new_static("self")))
                                    .with_annotated_type(instance_ty),
                                Parameter::positional_only(Some(Name::new_static("key")))
                                    .with_annotated_type(key_type),
                                Parameter::positional_only(Some(Name::new_static("default")))
                                    .with_annotated_type(field.declared_ty),
                            ],
                        ),
                        Some(field.declared_ty),
                    )
                });

                Some(Type::Callable(CallableType::new(
                    db,
                    CallableSignature::from_overloads(overloads),
                    CallableTypeKind::FunctionLike,
                )))
            }
            (CodeGeneratorKind::TypedDict, "update") => {
                // TODO: synthesize a set of overloads with precise types
                let signature = Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(instance_ty),
                            Parameter::variadic(Name::new_static("args")),
                            Parameter::keyword_variadic(Name::new_static("kwargs")),
                        ],
                    ),
                    Some(Type::none(db)),
                );

                Some(Type::function_like_callable(db, signature))
            }
            _ => None,
        }
    }

    /// Member lookup for classes that inherit from `typing.TypedDict`.
    ///
    /// This is implemented as a separate method because the item definitions on a `TypedDict`-based
    /// class are *not* accessible as class members. Instead, this mostly defers to `TypedDictFallback`,
    /// unless `name` corresponds to one of the specialized synthetic members like `__getitem__`.
    pub(crate) fn typed_dict_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        if let Some(member) = self.own_synthesized_member(db, specialization, None, name) {
            Place::bound(member).into()
        } else {
            KnownClass::TypedDictFallback
                .to_class_literal(db)
                .find_name_in_mro_with_policy(db, name, policy)
                .expect("`find_name_in_mro_with_policy` will return `Some()` when called on class literal")
                .map_type(|ty|
                    ty.apply_type_mapping(
                        db,
                        &TypeMapping::ReplaceSelf {
                            new_upper_bound: determine_upper_bound(
                                db,
                                self,
                                specialization,
                                ClassBase::is_typed_dict
                            )
                        },
                                TypeContext::default(),
                    )
                )
        }
    }

    /// Returns a list of all annotated attributes defined in this class, or any of its superclasses.
    ///
    /// See [`ClassLiteral::own_fields`] for more details.
    #[salsa::tracked(
        returns(ref),
        cycle_initial=fields_cycle_initial,
        heap_size=get_size2::GetSize::get_heap_size)]
    pub(crate) fn fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> FxIndexMap<Name, Field<'db>> {
        if field_policy == CodeGeneratorKind::NamedTuple {
            // NamedTuples do not allow multiple inheritance, so it is sufficient to enumerate the
            // fields of this class only.
            return self.own_fields(db, specialization, field_policy);
        }

        let matching_classes_in_mro: Vec<_> = self
            .iter_mro(db, specialization)
            .filter_map(|superclass| {
                if let Some(class) = superclass.into_class() {
                    let (class_literal, specialization) = class.class_literal(db);
                    if field_policy.matches(db, class_literal, specialization) {
                        Some((class_literal, specialization))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            // We need to collect into a `Vec` here because we iterate the MRO in reverse order
            .collect();

        matching_classes_in_mro
            .into_iter()
            .rev()
            .flat_map(|(class, specialization)| class.own_fields(db, specialization, field_policy))
            // We collect into a FxOrderMap here to deduplicate attributes
            .collect()
    }

    /// Returns a list of all annotated attributes defined in the body of this class. This is similar
    /// to the `__annotations__` attribute at runtime, but also contains default values.
    ///
    /// For a class body like
    /// ```py
    /// @dataclass
    /// class C:
    ///     x: int
    ///     y: str = "a"
    /// ```
    /// we return a map `{"x": (int, None), "y": (str, Some(Literal["a"]))}`.
    pub(super) fn own_fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind,
    ) -> FxIndexMap<Name, Field<'db>> {
        let mut attributes = FxIndexMap::default();

        let class_body_scope = self.body_scope(db);
        let table = place_table(db, class_body_scope);

        let use_def = use_def_map(db, class_body_scope);

        let typed_dict_params = self.typed_dict_params(db);
        let mut kw_only_sentinel_field_seen = false;

        for (symbol_id, declarations) in use_def.all_end_of_scope_symbol_declarations() {
            // Here, we exclude all declarations that are not annotated assignments. We need this because
            // things like function definitions and nested classes would otherwise be considered dataclass
            // fields. The check is too broad in the sense that it also excludes (weird) constructs where
            // a symbol would have multiple declarations, one of which is an annotated assignment. If we
            // want to improve this, we could instead pass a definition-kind filter to the use-def map
            // query, or to the `symbol_from_declarations` call below. Doing so would potentially require
            // us to generate a union of `__init__` methods.
            if !declarations
                .clone()
                .all(|DeclarationWithConstraint { declaration, .. }| {
                    declaration.is_undefined_or(|declaration| {
                        matches!(
                            declaration.kind(db),
                            DefinitionKind::AnnotatedAssignment(..)
                        )
                    })
                })
            {
                continue;
            }

            let symbol = table.symbol(symbol_id);

            let result = place_from_declarations(db, declarations.clone());
            let first_declaration = result.first_declaration;
            let attr = result.ignore_conflicting_declarations();
            if attr.is_class_var() {
                continue;
            }

            if let Some(attr_ty) = attr.place.ignore_possibly_undefined() {
                let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
                let mut default_ty = place_from_bindings(db, bindings)
                    .place
                    .ignore_possibly_undefined();

                default_ty =
                    default_ty.map(|ty| ty.apply_optional_specialization(db, specialization));

                let mut init = true;
                let mut kw_only = None;
                let mut alias = None;
                if let Some(Type::KnownInstance(KnownInstanceType::Field(field))) = default_ty {
                    default_ty = field.default_type(db);
                    if self
                        .dataclass_params(db)
                        .map(|params| params.field_specifiers(db).is_empty())
                        .unwrap_or(false)
                    {
                        // This happens when constructing a `dataclass` with a `dataclass_transform`
                        // without defining the `field_specifiers`, meaning it should ignore
                        // `dataclasses.field` and `dataclasses.Field`.
                    } else {
                        init = field.init(db);
                        kw_only = field.kw_only(db);
                        alias = field.alias(db);
                    }
                }

                let kind = match field_policy {
                    CodeGeneratorKind::NamedTuple => FieldKind::NamedTuple { default_ty },
                    CodeGeneratorKind::DataclassLike(_) => FieldKind::Dataclass {
                        default_ty,
                        init_only: attr.is_init_var(),
                        init,
                        kw_only,
                        alias,
                    },
                    CodeGeneratorKind::TypedDict => {
                        let is_required = if attr.is_required() {
                            // Explicit Required[T] annotation - always required
                            true
                        } else if attr.is_not_required() {
                            // Explicit NotRequired[T] annotation - never required
                            false
                        } else {
                            // No explicit qualifier - use class default (`total` parameter)
                            typed_dict_params
                                .expect("TypedDictParams should be available for CodeGeneratorKind::TypedDict")
                                .contains(TypedDictParams::TOTAL)
                        };

                        FieldKind::TypedDict {
                            is_required,
                            is_read_only: attr.is_read_only(),
                        }
                    }
                };

                let mut field = Field {
                    declared_ty: attr_ty.apply_optional_specialization(db, specialization),
                    kind,
                    first_declaration,
                };

                // Check if this is a KW_ONLY sentinel and mark subsequent fields as keyword-only
                if field.is_kw_only_sentinel(db) {
                    kw_only_sentinel_field_seen = true;
                }

                // If no explicit kw_only setting and we've seen KW_ONLY sentinel, mark as keyword-only
                if kw_only_sentinel_field_seen {
                    if let FieldKind::Dataclass {
                        kw_only: ref mut kw @ None,
                        ..
                    } = field.kind
                    {
                        *kw = Some(true);
                    }
                }

                attributes.insert(symbol.name().clone(), field);
            }
        }

        attributes
    }

    /// Look up an instance attribute (available in `__dict__`) of the given name.
    ///
    /// See [`Type::instance_member`] for more details.
    pub(super) fn instance_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        if self.is_typed_dict(db) {
            return Place::Undefined.into();
        }

        let mut union = UnionBuilder::new(db);
        let mut union_qualifiers = TypeQualifiers::empty();
        let mut is_definitely_bound = false;

        for superclass in self.iter_mro(db, specialization) {
            match superclass {
                ClassBase::Generic | ClassBase::Protocol => {
                    // Skip over these very special class bases that aren't really classes.
                }
                ClassBase::Dynamic(_) => {
                    return PlaceAndQualifiers::todo(
                        "instance attribute on class with dynamic base",
                    );
                }
                ClassBase::Class(class) => {
                    if let member @ PlaceAndQualifiers {
                        place: Place::Defined(ty, origin, boundness),
                        qualifiers,
                    } = class.own_instance_member(db, name).inner
                    {
                        if boundness == Definedness::AlwaysDefined {
                            if origin.is_declared() {
                                // We found a definitely-declared attribute. Discard possibly collected
                                // inferred types from subclasses and return the declared type.
                                return member;
                            }

                            is_definitely_bound = true;
                        }

                        // If the attribute is not definitely declared on this class, keep looking higher
                        // up in the MRO, and build a union of all inferred types (and possibly-declared
                        // types):
                        union = union.add(ty);

                        // TODO: We could raise a diagnostic here if there are conflicting type qualifiers
                        union_qualifiers |= qualifiers;
                    }
                }
                ClassBase::TypedDict => {
                    return KnownClass::TypedDictFallback
                        .to_instance(db)
                        .instance_member(db, name)
                        .map_type(|ty| {
                            ty.apply_type_mapping(
                                db,
                                &TypeMapping::ReplaceSelf {
                                    new_upper_bound: Type::instance(
                                        db,
                                        self.unknown_specialization(db),
                                    ),
                                },
                                TypeContext::default(),
                            )
                        });
                }
            }
        }

        if union.is_empty() {
            Place::Undefined.with_qualifiers(TypeQualifiers::empty())
        } else {
            let boundness = if is_definitely_bound {
                Definedness::AlwaysDefined
            } else {
                Definedness::PossiblyUndefined
            };

            Place::Defined(union.build(), TypeOrigin::Inferred, boundness)
                .with_qualifiers(union_qualifiers)
        }
    }

    /// Tries to find declarations/bindings of an attribute named `name` that are only
    /// "implicitly" defined (`self.x = …`, `cls.x = …`) in a method of the class that
    /// corresponds to `class_body_scope`. The `target_method_decorator` parameter is
    /// used to skip methods that do not have the expected decorator.
    fn implicit_attribute(
        db: &'db dyn Db,
        class_body_scope: ScopeId<'db>,
        name: &str,
        target_method_decorator: MethodDecorator,
    ) -> Member<'db> {
        Self::implicit_attribute_inner(
            db,
            class_body_scope,
            name.to_string(),
            target_method_decorator,
        )
    }

    #[salsa::tracked(
        cycle_fn=implicit_attribute_cycle_recover,
        cycle_initial=implicit_attribute_initial,
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(super) fn implicit_attribute_inner(
        db: &'db dyn Db,
        class_body_scope: ScopeId<'db>,
        name: String,
        target_method_decorator: MethodDecorator,
    ) -> Member<'db> {
        // If we do not see any declarations of an attribute, neither in the class body nor in
        // any method, we build a union of `Unknown` with the inferred types of all bindings of
        // that attribute. We include `Unknown` in that union to account for the fact that the
        // attribute might be externally modified.
        let mut union_of_inferred_types = UnionBuilder::new(db);
        let mut qualifiers = TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE;

        let mut is_attribute_bound = false;

        let file = class_body_scope.file(db);
        let module = parsed_module(db, file).load(db);
        let index = semantic_index(db, file);
        let class_map = use_def_map(db, class_body_scope);
        let class_table = place_table(db, class_body_scope);
        let is_valid_scope = |method_scope: &Scope| {
            if let Some(method_def) = method_scope.node().as_function() {
                let method_name = method_def.node(&module).name.as_str();
                if let Some(Type::FunctionLiteral(method_type)) =
                    class_member(db, class_body_scope, method_name)
                        .inner
                        .place
                        .ignore_possibly_undefined()
                {
                    let method_decorator = MethodDecorator::try_from_fn_type(db, method_type);
                    if method_decorator != Ok(target_method_decorator) {
                        return false;
                    }
                }
            }
            true
        };

        // First check declarations
        for (attribute_declarations, method_scope_id) in
            attribute_declarations(db, class_body_scope, &name)
        {
            let method_scope = index.scope(method_scope_id);
            if !is_valid_scope(method_scope) {
                continue;
            }

            for attribute_declaration in attribute_declarations {
                let DefinitionState::Defined(declaration) = attribute_declaration.declaration
                else {
                    continue;
                };

                let DefinitionKind::AnnotatedAssignment(assignment) = declaration.kind(db) else {
                    continue;
                };

                // We found an annotated assignment of one of the following forms (using 'self' in these
                // examples, but we support arbitrary names for the first parameters of methods):
                //
                //     self.name: <annotation>
                //     self.name: <annotation> = …

                let annotation = declaration_type(db, declaration);
                let annotation = Place::declared(annotation.inner).with_qualifiers(
                    annotation.qualifiers | TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE,
                );

                if let Some(all_qualifiers) = annotation.is_bare_final() {
                    if let Some(value) = assignment.value(&module) {
                        // If we see an annotated assignment with a bare `Final` as in
                        // `self.SOME_CONSTANT: Final = 1`, infer the type from the value
                        // on the right-hand side.

                        let inferred_ty = infer_expression_type(
                            db,
                            index.expression(value),
                            TypeContext::default(),
                        );
                        return Member {
                            inner: Place::bound(inferred_ty).with_qualifiers(all_qualifiers),
                        };
                    }

                    // If there is no right-hand side, just record that we saw a `Final` qualifier
                    qualifiers |= all_qualifiers;
                    continue;
                }

                return Member { inner: annotation };
            }
        }

        if !qualifiers.contains(TypeQualifiers::FINAL) {
            union_of_inferred_types = union_of_inferred_types.add(Type::unknown());
        }

        for (attribute_assignments, attribute_binding_scope_id) in
            attribute_assignments(db, class_body_scope, &name)
        {
            let binding_scope = index.scope(attribute_binding_scope_id);
            if !is_valid_scope(binding_scope) {
                continue;
            }

            let scope_for_reachability_analysis = {
                if binding_scope.node().as_function().is_some() {
                    binding_scope
                } else if binding_scope.is_eager() {
                    let mut eager_scope_parent = binding_scope;
                    while eager_scope_parent.is_eager()
                        && let Some(parent) = eager_scope_parent.parent()
                    {
                        eager_scope_parent = index.scope(parent);
                    }
                    eager_scope_parent
                } else {
                    binding_scope
                }
            };

            // The attribute assignment inherits the reachability of the method which contains it
            let is_method_reachable =
                if let Some(method_def) = scope_for_reachability_analysis.node().as_function() {
                    let method = index.expect_single_definition(method_def);
                    let method_place = class_table
                        .symbol_id(&method_def.node(&module).name)
                        .unwrap();
                    class_map
                        .all_reachable_symbol_bindings(method_place)
                        .find_map(|bind| {
                            (bind.binding.is_defined_and(|def| def == method))
                                .then(|| class_map.binding_reachability(db, &bind))
                        })
                        .unwrap_or(Truthiness::AlwaysFalse)
                } else {
                    Truthiness::AlwaysFalse
                };
            if is_method_reachable.is_always_false() {
                continue;
            }

            for attribute_assignment in attribute_assignments {
                if let DefinitionState::Undefined = attribute_assignment.binding {
                    continue;
                }

                let DefinitionState::Defined(binding) = attribute_assignment.binding else {
                    continue;
                };

                if !is_method_reachable.is_always_false() {
                    is_attribute_bound = true;
                }

                match binding.kind(db) {
                    DefinitionKind::AnnotatedAssignment(_) => {
                        // Annotated assignments were handled above. This branch is not
                        // unreachable (because of the `continue` above), but there is
                        // nothing to do here.
                    }
                    DefinitionKind::Assignment(assign) => {
                        match assign.target_kind() {
                            TargetKind::Sequence(_, unpack) => {
                                // We found an unpacking assignment like:
                                //
                                //     .., self.name, .. = <value>
                                //     (.., self.name, ..) = <value>
                                //     [.., self.name, ..] = <value>

                                let unpacked = infer_unpack_types(db, unpack);

                                let inferred_ty = unpacked.expression_type(assign.target(&module));

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                            TargetKind::Single => {
                                // We found an un-annotated attribute assignment of the form:
                                //
                                //     self.name = <value>

                                let inferred_ty = infer_expression_type(
                                    db,
                                    index.expression(assign.value(&module)),
                                    TypeContext::default(),
                                );

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                        }
                    }
                    DefinitionKind::For(for_stmt) => {
                        match for_stmt.target_kind() {
                            TargetKind::Sequence(_, unpack) => {
                                // We found an unpacking assignment like:
                                //
                                //     for .., self.name, .. in <iterable>:

                                let unpacked = infer_unpack_types(db, unpack);
                                let inferred_ty =
                                    unpacked.expression_type(for_stmt.target(&module));

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                            TargetKind::Single => {
                                // We found an attribute assignment like:
                                //
                                //     for self.name in <iterable>:

                                let iterable_ty = infer_expression_type(
                                    db,
                                    index.expression(for_stmt.iterable(&module)),
                                    TypeContext::default(),
                                );
                                // TODO: Potential diagnostics resulting from the iterable are currently not reported.
                                let inferred_ty =
                                    iterable_ty.iterate(db).homogeneous_element_type(db);

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                        }
                    }
                    DefinitionKind::WithItem(with_item) => {
                        match with_item.target_kind() {
                            TargetKind::Sequence(_, unpack) => {
                                // We found an unpacking assignment like:
                                //
                                //     with <context_manager> as .., self.name, ..:

                                let unpacked = infer_unpack_types(db, unpack);
                                let inferred_ty =
                                    unpacked.expression_type(with_item.target(&module));

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                            TargetKind::Single => {
                                // We found an attribute assignment like:
                                //
                                //     with <context_manager> as self.name:

                                let context_ty = infer_expression_type(
                                    db,
                                    index.expression(with_item.context_expr(&module)),
                                    TypeContext::default(),
                                );
                                let inferred_ty = if with_item.is_async() {
                                    context_ty.aenter(db)
                                } else {
                                    context_ty.enter(db)
                                };

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                        }
                    }
                    DefinitionKind::Comprehension(comprehension) => {
                        match comprehension.target_kind() {
                            TargetKind::Sequence(_, unpack) => {
                                // We found an unpacking assignment like:
                                //
                                //     [... for .., self.name, .. in <iterable>]

                                let unpacked = infer_unpack_types(db, unpack);

                                let inferred_ty =
                                    unpacked.expression_type(comprehension.target(&module));

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                            TargetKind::Single => {
                                // We found an attribute assignment like:
                                //
                                //     [... for self.name in <iterable>]

                                let iterable_ty = infer_expression_type(
                                    db,
                                    index.expression(comprehension.iterable(&module)),
                                    TypeContext::default(),
                                );
                                // TODO: Potential diagnostics resulting from the iterable are currently not reported.
                                let inferred_ty =
                                    iterable_ty.iterate(db).homogeneous_element_type(db);

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                        }
                    }
                    DefinitionKind::AugmentedAssignment(_) => {
                        // TODO:
                    }
                    DefinitionKind::NamedExpression(_) => {
                        // A named expression whose target is an attribute is syntactically prohibited
                    }
                    _ => {}
                }
            }
        }

        Member {
            inner: if is_attribute_bound {
                Place::bound(union_of_inferred_types.build()).with_qualifiers(qualifiers)
            } else {
                Place::Undefined.with_qualifiers(qualifiers)
            },
        }
    }

    /// A helper function for `instance_member` that looks up the `name` attribute only on
    /// this class, not on its superclasses.
    fn own_instance_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        // TODO: There are many things that are not yet implemented here:
        // - `typing.Final`
        // - Proper diagnostics

        let body_scope = self.body_scope(db);
        let table = place_table(db, body_scope);

        if let Some(symbol_id) = table.symbol_id(name) {
            let use_def = use_def_map(db, body_scope);

            let declarations = use_def.end_of_scope_symbol_declarations(symbol_id);
            let declared_and_qualifiers =
                place_from_declarations(db, declarations).ignore_conflicting_declarations();

            match declared_and_qualifiers {
                PlaceAndQualifiers {
                    place: mut declared @ Place::Defined(declared_ty, _, declaredness),
                    qualifiers,
                } => {
                    // For the purpose of finding instance attributes, ignore `ClassVar`
                    // declarations:
                    if qualifiers.contains(TypeQualifiers::CLASS_VAR) {
                        declared = Place::Undefined;
                    }

                    if qualifiers.contains(TypeQualifiers::INIT_VAR) {
                        // We ignore `InitVar` declarations on the class body, unless that attribute is overwritten
                        // by an implicit assignment in a method
                        if Self::implicit_attribute(db, body_scope, name, MethodDecorator::None)
                            .is_undefined()
                        {
                            return Member::unbound();
                        }
                    }

                    // The attribute is declared in the class body.

                    let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
                    let inferred = place_from_bindings(db, bindings).place;
                    let has_binding = !inferred.is_undefined();

                    if has_binding {
                        // The attribute is declared and bound in the class body.

                        if let Some(implicit_ty) =
                            Self::implicit_attribute(db, body_scope, name, MethodDecorator::None)
                                .ignore_possibly_undefined()
                        {
                            if declaredness == Definedness::AlwaysDefined {
                                // If a symbol is definitely declared, and we see
                                // attribute assignments in methods of the class,
                                // we trust the declared type.
                                Member {
                                    inner: declared.with_qualifiers(qualifiers),
                                }
                            } else {
                                Member {
                                    inner: Place::Defined(
                                        UnionType::from_elements(db, [declared_ty, implicit_ty]),
                                        TypeOrigin::Declared,
                                        declaredness,
                                    )
                                    .with_qualifiers(qualifiers),
                                }
                            }
                        } else {
                            // The symbol is declared and bound in the class body,
                            // but we did not find any attribute assignments in
                            // methods of the class. This means that the attribute
                            // has a class-level default value, but it would not be
                            // found in a `__dict__` lookup.

                            Member::unbound()
                        }
                    } else {
                        // The attribute is declared but not bound in the class body.
                        // We take this as a sign that this is intended to be a pure
                        // instance attribute, and we trust the declared type, unless
                        // it is possibly-undeclared. In the latter case, we also
                        // union with the inferred type from attribute assignments.

                        if declaredness == Definedness::AlwaysDefined {
                            Member {
                                inner: declared.with_qualifiers(qualifiers),
                            }
                        } else {
                            if let Some(implicit_ty) = Self::implicit_attribute(
                                db,
                                body_scope,
                                name,
                                MethodDecorator::None,
                            )
                            .inner
                            .place
                            .ignore_possibly_undefined()
                            {
                                Member {
                                    inner: Place::Defined(
                                        UnionType::from_elements(db, [declared_ty, implicit_ty]),
                                        TypeOrigin::Declared,
                                        declaredness,
                                    )
                                    .with_qualifiers(qualifiers),
                                }
                            } else {
                                Member {
                                    inner: declared.with_qualifiers(qualifiers),
                                }
                            }
                        }
                    }
                }

                PlaceAndQualifiers {
                    place: Place::Undefined,
                    qualifiers: _,
                } => {
                    // The attribute is not *declared* in the class body. It could still be declared/bound
                    // in a method.

                    Self::implicit_attribute(db, body_scope, name, MethodDecorator::None)
                }
            }
        } else {
            // This attribute is neither declared nor bound in the class body.
            // It could still be implicitly defined in a method.

            Self::implicit_attribute(db, body_scope, name, MethodDecorator::None)
        }
    }

    pub(super) fn to_non_generic_instance(self, db: &'db dyn Db) -> Type<'db> {
        Type::instance(db, ClassType::NonGeneric(self))
    }

    /// Return this class' involvement in an inheritance cycle, if any.
    ///
    /// A class definition like this will fail at runtime,
    /// but we must be resilient to it or we could panic.
    #[salsa::tracked(cycle_initial=inheritance_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(super) fn inheritance_cycle(self, db: &'db dyn Db) -> Option<InheritanceCycle> {
        /// Return `true` if the class is cyclically defined.
        ///
        /// Also, populates `visited_classes` with all base classes of `self`.
        fn is_cyclically_defined_recursive<'db>(
            db: &'db dyn Db,
            class: ClassLiteral<'db>,
            classes_on_stack: &mut IndexSet<ClassLiteral<'db>>,
            visited_classes: &mut IndexSet<ClassLiteral<'db>>,
        ) -> bool {
            let mut result = false;
            for explicit_base in class.explicit_bases(db) {
                let explicit_base_class_literal = match explicit_base {
                    Type::ClassLiteral(class_literal) => *class_literal,
                    Type::GenericAlias(generic_alias) => generic_alias.origin(db),
                    _ => continue,
                };
                if !classes_on_stack.insert(explicit_base_class_literal) {
                    return true;
                }

                if visited_classes.insert(explicit_base_class_literal) {
                    // If we find a cycle, keep searching to check if we can reach the starting class.
                    result |= is_cyclically_defined_recursive(
                        db,
                        explicit_base_class_literal,
                        classes_on_stack,
                        visited_classes,
                    );
                }
                classes_on_stack.pop();
            }
            result
        }

        tracing::trace!("Class::inheritance_cycle: {}", self.name(db));

        let visited_classes = &mut IndexSet::new();
        if !is_cyclically_defined_recursive(db, self, &mut IndexSet::new(), visited_classes) {
            None
        } else if visited_classes.contains(&self) {
            Some(InheritanceCycle::Participant)
        } else {
            Some(InheritanceCycle::Inherited)
        }
    }

    /// Returns a [`Span`] with the range of the class's header.
    ///
    /// See [`Self::header_range`] for more details.
    pub(super) fn header_span(self, db: &'db dyn Db) -> Span {
        Span::from(self.file(db)).with_range(self.header_range(db))
    }

    /// Returns the range of the class's "header": the class name
    /// and any arguments passed to the `class` statement. E.g.
    ///
    /// ```ignore
    /// class Foo(Bar, metaclass=Baz): ...
    ///       ^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub(super) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let class_scope = self.body_scope(db);
        let module = parsed_module(db, class_scope.file(db)).load(db);
        let class_node = class_scope.node(db).expect_class().node(&module);
        let class_name = &class_node.name;
        TextRange::new(
            class_name.start(),
            class_node
                .arguments
                .as_deref()
                .map(Ranged::end)
                .unwrap_or_else(|| class_name.end()),
        )
    }

    pub(super) fn qualified_name(self, db: &'db dyn Db) -> QualifiedClassName<'db> {
        QualifiedClassName { db, class: self }
    }
}

impl<'db> From<ClassLiteral<'db>> for Type<'db> {
    fn from(class: ClassLiteral<'db>) -> Type<'db> {
        Type::ClassLiteral(class)
    }
}

#[salsa::tracked]
impl<'db> VarianceInferable<'db> for ClassLiteral<'db> {
    #[salsa::tracked(cycle_initial=crate::types::variance_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        let typevar_in_generic_context = self
            .generic_context(db)
            .is_some_and(|generic_context| generic_context.variables(db).contains(&typevar));

        if !typevar_in_generic_context {
            return TypeVarVariance::Bivariant;
        }
        let class_body_scope = self.body_scope(db);

        let file = class_body_scope.file(db);
        let index = semantic_index(db, file);

        let explicit_bases_variances = self
            .explicit_bases(db)
            .iter()
            .map(|class| class.variance_of(db, typevar));

        let default_attribute_variance = {
            let is_namedtuple = CodeGeneratorKind::NamedTuple.matches(db, self, None);
            // Python 3.13 introduced a synthesized `__replace__` method on dataclasses which uses
            // their field types in contravariant position, thus meaning a frozen dataclass must
            // still be invariant in its field types. Other synthesized methods on dataclasses are
            // not considered here, since they don't use field types in their signatures. TODO:
            // ideally we'd have a single source of truth for information about synthesized
            // methods, so we just look them up normally and don't hardcode this knowledge here.
            let is_frozen_dataclass = Program::get(db).python_version(db) <= PythonVersion::PY312
                && self
                    .dataclass_params(db)
                    .is_some_and(|params| params.flags(db).contains(DataclassFlags::FROZEN));
            if is_namedtuple || is_frozen_dataclass {
                TypeVarVariance::Covariant
            } else {
                TypeVarVariance::Invariant
            }
        };

        let init_name: &Name = &"__init__".into();
        let new_name: &Name = &"__new__".into();

        let use_def_map = index.use_def_map(class_body_scope.file_scope_id(db));
        let table = place_table(db, class_body_scope);
        let attribute_places_and_qualifiers =
            use_def_map
                .all_end_of_scope_symbol_declarations()
                .map(|(symbol_id, declarations)| {
                    let place_and_qual =
                        place_from_declarations(db, declarations).ignore_conflicting_declarations();
                    (symbol_id, place_and_qual)
                })
                .chain(use_def_map.all_end_of_scope_symbol_bindings().map(
                    |(symbol_id, bindings)| {
                        (symbol_id, place_from_bindings(db, bindings).place.into())
                    },
                ))
                .filter_map(|(symbol_id, place_and_qual)| {
                    if let Some(name) = table.place(symbol_id).as_symbol().map(Symbol::name) {
                        (![init_name, new_name].contains(&name))
                            .then_some((name.to_string(), place_and_qual))
                    } else {
                        None
                    }
                });

        // Dataclasses can have some additional synthesized methods (`__eq__`, `__hash__`,
        // `__lt__`, etc.) but none of these will have field types type variables in their signatures, so we
        // don't need to consider them for variance.

        let attribute_names = attribute_scopes(db, self.body_scope(db))
            .flat_map(|function_scope_id| {
                index
                    .place_table(function_scope_id)
                    .members()
                    .filter_map(|member| member.as_instance_attribute())
                    .filter(|name| *name != init_name && *name != new_name)
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .dedup();

        let attribute_variances = attribute_names
            .map(|name| {
                let place_and_quals = self.own_instance_member(db, &name).inner;
                (name, place_and_quals)
            })
            .chain(attribute_places_and_qualifiers)
            .dedup()
            .filter_map(|(name, place_and_qual)| {
                place_and_qual.ignore_possibly_undefined().map(|ty| {
                    let variance = if place_and_qual
                        .qualifiers
                        // `CLASS_VAR || FINAL` is really `all()`, but
                        // we want to be robust against new qualifiers
                        .intersects(TypeQualifiers::CLASS_VAR | TypeQualifiers::FINAL)
                        // We don't allow mutation of methods or properties
                        || ty.is_function_literal()
                        || ty.is_property_instance()
                        // Underscore-prefixed attributes are assumed not to be externally mutated
                        || name.starts_with('_')
                    {
                        // CLASS_VAR: class vars generally shouldn't contain the
                        // type variable, but they could if it's a
                        // callable type. They can't be mutated on instances.
                        //
                        // FINAL: final attributes are immutable, and thus covariant
                        TypeVarVariance::Covariant
                    } else {
                        default_attribute_variance
                    };
                    ty.with_polarity(variance).variance_of(db, typevar)
                })
            });

        attribute_variances
            .chain(explicit_bases_variances)
            .collect()
    }
}

// N.B. It would be incorrect to derive `Eq`, `PartialEq`, or `Hash` for this struct,
// because two `QualifiedClassName` instances might refer to different classes but
// have the same components. You'd expect them to compare equal, but they'd compare
// unequal if `PartialEq`/`Eq` were naively derived.
#[derive(Clone, Copy)]
pub(super) struct QualifiedClassName<'db> {
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
}

impl QualifiedClassName<'_> {
    /// Returns the components of the qualified name of this class, excluding this class itself.
    ///
    /// For example, calling this method on a class `C` in the module `a.b` would return
    /// `["a", "b"]`. Calling this method on a class `D` inside the namespace of a method
    /// `m` inside the namespace of a class `C` in the module `a.b` would return
    /// `["a", "b", "C", "<locals of function 'm'>"]`.
    pub(super) fn components_excluding_self(&self) -> Vec<String> {
        let body_scope = self.class.body_scope(self.db);
        let file = body_scope.file(self.db);
        let module_ast = parsed_module(self.db, file).load(self.db);
        let index = semantic_index(self.db, file);
        let file_scope_id = body_scope.file_scope_id(self.db);

        let mut name_parts = vec![];

        // Skips itself
        for (_, ancestor_scope) in index.ancestor_scopes(file_scope_id).skip(1) {
            let node = ancestor_scope.node();

            match ancestor_scope.kind() {
                ScopeKind::Class => {
                    if let Some(class_def) = node.as_class() {
                        name_parts.push(class_def.node(&module_ast).name.as_str().to_string());
                    }
                }
                ScopeKind::Function => {
                    if let Some(function_def) = node.as_function() {
                        name_parts.push(format!(
                            "<locals of function '{}'>",
                            function_def.node(&module_ast).name.as_str()
                        ));
                    }
                }
                _ => {}
            }
        }

        if let Some(module) = file_to_module(self.db, file) {
            let module_name = module.name(self.db);
            name_parts.push(module_name.as_str().to_string());
        }

        name_parts.reverse();
        name_parts
    }
}

impl std::fmt::Display for QualifiedClassName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for parent in self.components_excluding_self() {
            f.write_str(&parent)?;
            f.write_char('.')?;
        }
        f.write_str(self.class.name(self.db))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(super) enum InheritanceCycle {
    /// The class is cyclically defined and is a participant in the cycle.
    /// i.e., it inherits either directly or indirectly from itself.
    Participant,
    /// The class inherits from a class that is a `Participant` in an inheritance cycle,
    /// but is not itself a participant.
    Inherited,
}

impl InheritanceCycle {
    pub(super) const fn is_participant(self) -> bool {
        matches!(self, InheritanceCycle::Participant)
    }
}

/// CPython internally considers a class a "solid base" if it has an atypical instance memory layout,
/// with additional memory "slots" for each instance, besides the default object metadata and an
/// attribute dictionary. Per [PEP 800], however, we use the term "disjoint base" for this concept.
///
/// A "disjoint base" can be a class defined in a C extension which defines C-level instance slots,
/// or a Python class that defines non-empty `__slots__`. C-level instance slots are not generally
/// visible to Python code, but PEP 800 specifies that any class decorated with
/// `@typing_extensions.disjoint_base` should be treated by type checkers as a disjoint base; it is
/// assumed that classes with C-level instance slots will be decorated as such when they appear in
/// stub files.
///
/// Two disjoint bases can only coexist in a class's MRO if one is a subclass of the other. Knowing if
/// a class is "disjoint base" or not is therefore valuable for inferring whether two instance types or
/// two subclass-of types are disjoint from each other. It also allows us to detect possible
/// `TypeError`s resulting from class definitions.
///
/// [PEP 800]: https://peps.python.org/pep-0800/
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub(super) struct DisjointBase<'db> {
    pub(super) class: ClassLiteral<'db>,
    pub(super) kind: DisjointBaseKind,
}

impl<'db> DisjointBase<'db> {
    /// Creates a [`DisjointBase`] instance where we know the class is a disjoint base
    /// because it has the `@disjoint_base` decorator on its definition
    fn due_to_decorator(class: ClassLiteral<'db>) -> Self {
        Self {
            class,
            kind: DisjointBaseKind::DisjointBaseDecorator,
        }
    }

    /// Creates a [`DisjointBase`] instance where we know the class is a disjoint base
    /// because of its `__slots__` definition.
    fn due_to_dunder_slots(class: ClassLiteral<'db>) -> Self {
        Self {
            class,
            kind: DisjointBaseKind::DefinesSlots,
        }
    }

    /// Two disjoint bases can only coexist in a class's MRO if one is a subclass of the other
    fn could_coexist_in_mro_with(&self, db: &'db dyn Db, other: &Self) -> bool {
        self == other
            || self
                .class
                .is_subclass_of(db, None, other.class.default_specialization(db))
            || other
                .class
                .is_subclass_of(db, None, self.class.default_specialization(db))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(super) enum DisjointBaseKind {
    /// We know the class is a disjoint base because it's either hardcoded in ty
    /// or has the `@disjoint_base` decorator.
    DisjointBaseDecorator,
    /// We know the class is a disjoint base because it has a non-empty `__slots__` definition.
    DefinesSlots,
}

/// Non-exhaustive enumeration of known classes (e.g. `builtins.int`, `typing.Any`, ...) to allow
/// for easier syntax when interacting with very common classes.
///
/// Feel free to expand this enum if you ever find yourself using the same class in multiple
/// places.
/// Note: good candidates are any classes in `[crate::module_resolver::module::KnownModule]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize)]
#[cfg_attr(test, derive(strum_macros::EnumIter))]
pub enum KnownClass {
    // To figure out where an stdlib symbol is defined, you can go into `crates/ty_vendored`
    // and grep for the symbol name in any `.pyi` file.

    // Builtins
    Bool,
    Object,
    Bytes,
    Bytearray,
    Type,
    Int,
    Float,
    Complex,
    Str,
    List,
    Tuple,
    Set,
    FrozenSet,
    Dict,
    Slice,
    Property,
    BaseException,
    Exception,
    BaseExceptionGroup,
    ExceptionGroup,
    Staticmethod,
    Classmethod,
    Super,
    // enum
    Enum,
    EnumType,
    Auto,
    Member,
    Nonmember,
    StrEnum,
    // abc
    ABCMeta,
    // Types
    GenericAlias,
    ModuleType,
    FunctionType,
    MethodType,
    MethodWrapperType,
    WrapperDescriptorType,
    UnionType,
    GeneratorType,
    AsyncGeneratorType,
    CoroutineType,
    NotImplementedType,
    BuiltinFunctionType,
    // Exposed as `types.EllipsisType` on Python >=3.10;
    // backported as `builtins.ellipsis` by typeshed on Python <=3.9
    EllipsisType,
    // Typeshed
    NoneType, // Part of `types` for Python >= 3.10
    // Typing
    Awaitable,
    Generator,
    Deprecated,
    StdlibAlias,
    SpecialForm,
    TypeVar,
    ParamSpec,
    ParamSpecArgs,
    ParamSpecKwargs,
    ProtocolMeta,
    TypeVarTuple,
    TypeAliasType,
    NoDefaultType,
    NewType,
    SupportsIndex,
    Iterable,
    Iterator,
    Mapping,
    // typing_extensions
    ExtensionsTypeVar, // must be distinct from typing.TypeVar, backports new features
    // Collections
    ChainMap,
    Counter,
    DefaultDict,
    Deque,
    OrderedDict,
    // sys
    VersionInfo,
    // dataclasses
    Field,
    KwOnly,
    InitVar,
    // _typeshed._type_checker_internals
    NamedTupleFallback,
    NamedTupleLike,
    TypedDictFallback,
    // string.templatelib
    Template,
    // pathlib
    Path,
    // ty_extensions
    ConstraintSet,
    GenericContext,
    Specialization,
}

impl KnownClass {
    pub(crate) const fn is_bool(self) -> bool {
        matches!(self, Self::Bool)
    }

    pub(crate) const fn is_special_form(self) -> bool {
        matches!(self, Self::SpecialForm)
    }

    /// Determine whether instances of this class are always truthy, always falsy,
    /// or have an ambiguous truthiness.
    ///
    /// Returns `None` for `KnownClass::Tuple`, since the truthiness of a tuple
    /// depends on its spec.
    pub(crate) const fn bool(self) -> Option<Truthiness> {
        match self {
            // N.B. It's only generally safe to infer `Truthiness::AlwaysTrue` for a `KnownClass`
            // variant if the class's `__bool__` method always returns the same thing *and* the
            // class is `@final`.
            //
            // E.g. `ModuleType.__bool__` always returns `True`, but `ModuleType` is not `@final`.
            // Equally, `range` is `@final`, but its `__bool__` method can return `False`.
            Self::EllipsisType
            | Self::NoDefaultType
            | Self::MethodType
            | Self::Slice
            | Self::FunctionType
            | Self::VersionInfo
            | Self::TypeAliasType
            | Self::TypeVar
            | Self::ExtensionsTypeVar
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Super
            | Self::WrapperDescriptorType
            | Self::UnionType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::MethodWrapperType
            | Self::CoroutineType
            | Self::BuiltinFunctionType
            | Self::Template
            | Self::Path => Some(Truthiness::AlwaysTrue),

            Self::NoneType => Some(Truthiness::AlwaysFalse),

            Self::BaseException
            | Self::Exception
            | Self::ExceptionGroup
            | Self::Object
            | Self::OrderedDict
            | Self::BaseExceptionGroup
            | Self::Bool
            | Self::Str
            | Self::List
            | Self::GenericAlias
            | Self::NewType
            | Self::StdlibAlias
            | Self::SupportsIndex
            | Self::Set
            | Self::Int
            | Self::Type
            | Self::Bytes
            | Self::Bytearray
            | Self::FrozenSet
            | Self::Property
            | Self::SpecialForm
            | Self::Dict
            | Self::ModuleType
            | Self::ChainMap
            | Self::Complex
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::Float
            | Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::ABCMeta
            | Self::Iterable
            | Self::Iterator
            | Self::Mapping
            // Evaluating `NotImplementedType` in a boolean context was deprecated in Python 3.9
            // and raises a `TypeError` in Python >=3.14
            // (see https://docs.python.org/3/library/constants.html#NotImplemented)
            | Self::NotImplementedType
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Awaitable
            | Self::Generator
            | Self::Deprecated
            | Self::Field
            | Self::KwOnly
            | Self::InitVar
            | Self::NamedTupleFallback
            | Self::NamedTupleLike
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization
            | Self::ProtocolMeta
            | Self::TypedDictFallback => Some(Truthiness::Ambiguous),

            Self::Tuple => None,
        }
    }

    /// Return `true` if this class is a subclass of `enum.Enum` *and* has enum members, i.e.
    /// if it is an "actual" enum, not `enum.Enum` itself or a similar custom enum class.
    pub(crate) const fn is_enum_subclass_with_members(self) -> bool {
        match self {
            KnownClass::Bool
            | KnownClass::Object
            | KnownClass::Bytes
            | KnownClass::Bytearray
            | KnownClass::Type
            | KnownClass::Int
            | KnownClass::Float
            | KnownClass::Complex
            | KnownClass::Str
            | KnownClass::List
            | KnownClass::Tuple
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Dict
            | KnownClass::Slice
            | KnownClass::Property
            | KnownClass::BaseException
            | KnownClass::Exception
            | KnownClass::BaseExceptionGroup
            | KnownClass::ExceptionGroup
            | KnownClass::Staticmethod
            | KnownClass::Classmethod
            | KnownClass::Awaitable
            | KnownClass::Generator
            | KnownClass::Deprecated
            | KnownClass::Super
            | KnownClass::Enum
            | KnownClass::EnumType
            | KnownClass::Auto
            | KnownClass::Member
            | KnownClass::Nonmember
            | KnownClass::StrEnum
            | KnownClass::ABCMeta
            | KnownClass::GenericAlias
            | KnownClass::ModuleType
            | KnownClass::FunctionType
            | KnownClass::MethodType
            | KnownClass::MethodWrapperType
            | KnownClass::WrapperDescriptorType
            | KnownClass::UnionType
            | KnownClass::GeneratorType
            | KnownClass::AsyncGeneratorType
            | KnownClass::CoroutineType
            | KnownClass::NoneType
            | KnownClass::StdlibAlias
            | KnownClass::SpecialForm
            | KnownClass::TypeVar
            | KnownClass::ExtensionsTypeVar
            | KnownClass::ParamSpec
            | KnownClass::ParamSpecArgs
            | KnownClass::ParamSpecKwargs
            | KnownClass::TypeVarTuple
            | KnownClass::TypeAliasType
            | KnownClass::NoDefaultType
            | KnownClass::NewType
            | KnownClass::SupportsIndex
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::Mapping
            | KnownClass::ChainMap
            | KnownClass::Counter
            | KnownClass::DefaultDict
            | KnownClass::Deque
            | KnownClass::OrderedDict
            | KnownClass::VersionInfo
            | KnownClass::EllipsisType
            | KnownClass::NotImplementedType
            | KnownClass::Field
            | KnownClass::KwOnly
            | KnownClass::InitVar
            | KnownClass::NamedTupleFallback
            | KnownClass::NamedTupleLike
            | KnownClass::ConstraintSet
            | KnownClass::GenericContext
            | KnownClass::Specialization
            | KnownClass::TypedDictFallback
            | KnownClass::BuiltinFunctionType
            | KnownClass::ProtocolMeta
            | KnownClass::Template
            | KnownClass::Path => false,
        }
    }

    /// Return `true` if this class is a (true) subclass of `typing.TypedDict`.
    pub(crate) const fn is_typed_dict_subclass(self) -> bool {
        match self {
            KnownClass::Bool
            | KnownClass::Object
            | KnownClass::Bytes
            | KnownClass::Bytearray
            | KnownClass::Type
            | KnownClass::Int
            | KnownClass::Float
            | KnownClass::Complex
            | KnownClass::Str
            | KnownClass::List
            | KnownClass::Tuple
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Dict
            | KnownClass::Slice
            | KnownClass::Property
            | KnownClass::BaseException
            | KnownClass::Exception
            | KnownClass::BaseExceptionGroup
            | KnownClass::ExceptionGroup
            | KnownClass::Staticmethod
            | KnownClass::Classmethod
            | KnownClass::Awaitable
            | KnownClass::Generator
            | KnownClass::Deprecated
            | KnownClass::Super
            | KnownClass::Enum
            | KnownClass::EnumType
            | KnownClass::Auto
            | KnownClass::Member
            | KnownClass::Nonmember
            | KnownClass::StrEnum
            | KnownClass::ABCMeta
            | KnownClass::GenericAlias
            | KnownClass::ModuleType
            | KnownClass::FunctionType
            | KnownClass::MethodType
            | KnownClass::MethodWrapperType
            | KnownClass::WrapperDescriptorType
            | KnownClass::UnionType
            | KnownClass::GeneratorType
            | KnownClass::AsyncGeneratorType
            | KnownClass::CoroutineType
            | KnownClass::NoneType
            | KnownClass::StdlibAlias
            | KnownClass::SpecialForm
            | KnownClass::TypeVar
            | KnownClass::ExtensionsTypeVar
            | KnownClass::ParamSpec
            | KnownClass::ParamSpecArgs
            | KnownClass::ParamSpecKwargs
            | KnownClass::TypeVarTuple
            | KnownClass::TypeAliasType
            | KnownClass::NoDefaultType
            | KnownClass::NewType
            | KnownClass::SupportsIndex
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::Mapping
            | KnownClass::ChainMap
            | KnownClass::Counter
            | KnownClass::DefaultDict
            | KnownClass::Deque
            | KnownClass::OrderedDict
            | KnownClass::VersionInfo
            | KnownClass::EllipsisType
            | KnownClass::NotImplementedType
            | KnownClass::Field
            | KnownClass::KwOnly
            | KnownClass::InitVar
            | KnownClass::NamedTupleFallback
            | KnownClass::NamedTupleLike
            | KnownClass::ConstraintSet
            | KnownClass::GenericContext
            | KnownClass::Specialization
            | KnownClass::TypedDictFallback
            | KnownClass::BuiltinFunctionType
            | KnownClass::ProtocolMeta
            | KnownClass::Template
            | KnownClass::Path => false,
        }
    }

    pub(crate) const fn is_tuple_subclass(self) -> bool {
        match self {
            KnownClass::Tuple | KnownClass::VersionInfo => true,

            KnownClass::Bool
            | KnownClass::Object
            | KnownClass::Bytes
            | KnownClass::Bytearray
            | KnownClass::Type
            | KnownClass::Int
            | KnownClass::Float
            | KnownClass::Complex
            | KnownClass::Str
            | KnownClass::List
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Dict
            | KnownClass::Slice
            | KnownClass::Property
            | KnownClass::BaseException
            | KnownClass::Exception
            | KnownClass::BaseExceptionGroup
            | KnownClass::ExceptionGroup
            | KnownClass::Staticmethod
            | KnownClass::Classmethod
            | KnownClass::Awaitable
            | KnownClass::Generator
            | KnownClass::Deprecated
            | KnownClass::Super
            | KnownClass::Enum
            | KnownClass::EnumType
            | KnownClass::Auto
            | KnownClass::Member
            | KnownClass::Nonmember
            | KnownClass::StrEnum
            | KnownClass::ABCMeta
            | KnownClass::GenericAlias
            | KnownClass::ModuleType
            | KnownClass::FunctionType
            | KnownClass::MethodType
            | KnownClass::MethodWrapperType
            | KnownClass::WrapperDescriptorType
            | KnownClass::UnionType
            | KnownClass::GeneratorType
            | KnownClass::AsyncGeneratorType
            | KnownClass::CoroutineType
            | KnownClass::NoneType
            | KnownClass::StdlibAlias
            | KnownClass::SpecialForm
            | KnownClass::TypeVar
            | KnownClass::ExtensionsTypeVar
            | KnownClass::ParamSpec
            | KnownClass::ParamSpecArgs
            | KnownClass::ParamSpecKwargs
            | KnownClass::TypeVarTuple
            | KnownClass::TypeAliasType
            | KnownClass::NoDefaultType
            | KnownClass::NewType
            | KnownClass::SupportsIndex
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::Mapping
            | KnownClass::ChainMap
            | KnownClass::Counter
            | KnownClass::DefaultDict
            | KnownClass::Deque
            | KnownClass::OrderedDict
            | KnownClass::EllipsisType
            | KnownClass::NotImplementedType
            | KnownClass::Field
            | KnownClass::KwOnly
            | KnownClass::InitVar
            | KnownClass::TypedDictFallback
            | KnownClass::NamedTupleLike
            | KnownClass::NamedTupleFallback
            | KnownClass::ConstraintSet
            | KnownClass::GenericContext
            | KnownClass::Specialization
            | KnownClass::BuiltinFunctionType
            | KnownClass::ProtocolMeta
            | KnownClass::Template
            | KnownClass::Path => false,
        }
    }

    /// Return `true` if this class is a protocol class.
    ///
    /// In an ideal world, perhaps we wouldn't hardcode this knowledge here;
    /// instead, we'd just look at the bases for these classes, as we do for
    /// all other classes. However, the special casing here helps us out in
    /// two important ways:
    ///
    /// 1. It helps us avoid Salsa cycles when creating types such as "instance of `str`"
    ///    and "instance of `sys._version_info`". These types are constructed very early
    ///    on, but it causes problems if we attempt to infer the types of their bases
    ///    too soon.
    /// 2. It's probably more performant.
    const fn is_protocol(self) -> bool {
        match self {
            Self::SupportsIndex
            | Self::Iterable
            | Self::Iterator
            | Self::Awaitable
            | Self::NamedTupleLike
            | Self::Generator => true,

            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
            | Self::Tuple
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::FrozenSet
            | Self::Str
            | Self::Set
            | Self::Dict
            | Self::List
            | Self::Type
            | Self::Slice
            | Self::Property
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Exception
            | Self::ExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Deprecated
            | Self::GenericAlias
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::CoroutineType
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::WrapperDescriptorType
            | Self::NoneType
            | Self::SpecialForm
            | Self::TypeVar
            | Self::ExtensionsTypeVar
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::TypeAliasType
            | Self::NoDefaultType
            | Self::NewType
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::ABCMeta
            | Self::Super
            | Self::StdlibAlias
            | Self::VersionInfo
            | Self::EllipsisType
            | Self::NotImplementedType
            | Self::UnionType
            | Self::Field
            | Self::KwOnly
            | Self::InitVar
            | Self::NamedTupleFallback
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization
            | Self::TypedDictFallback
            | Self::BuiltinFunctionType
            | Self::ProtocolMeta
            | Self::Template
            | Self::Path
            | Self::Mapping => false,
        }
    }

    /// Return `true` if this class is a typeshed fallback class which is used to provide attributes and
    /// methods for another type (e.g. `NamedTupleFallback` for actual `NamedTuple`s). These fallback
    /// classes need special treatment in some places. For example, implicit usages of `Self` should not
    /// be eagerly replaced with the fallback class itself. Instead, `Self` should eventually be treated
    /// as referring to the destination type (e.g. the actual `NamedTuple`).
    pub(crate) const fn is_fallback_class(self) -> bool {
        match self {
            KnownClass::Bool
            | KnownClass::Object
            | KnownClass::Bytes
            | KnownClass::Bytearray
            | KnownClass::Type
            | KnownClass::Int
            | KnownClass::Float
            | KnownClass::Complex
            | KnownClass::Str
            | KnownClass::List
            | KnownClass::Tuple
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Dict
            | KnownClass::Slice
            | KnownClass::Property
            | KnownClass::BaseException
            | KnownClass::Exception
            | KnownClass::BaseExceptionGroup
            | KnownClass::ExceptionGroup
            | KnownClass::Staticmethod
            | KnownClass::Classmethod
            | KnownClass::Super
            | KnownClass::Enum
            | KnownClass::EnumType
            | KnownClass::Auto
            | KnownClass::Member
            | KnownClass::Nonmember
            | KnownClass::StrEnum
            | KnownClass::ABCMeta
            | KnownClass::GenericAlias
            | KnownClass::ModuleType
            | KnownClass::FunctionType
            | KnownClass::MethodType
            | KnownClass::MethodWrapperType
            | KnownClass::WrapperDescriptorType
            | KnownClass::UnionType
            | KnownClass::GeneratorType
            | KnownClass::AsyncGeneratorType
            | KnownClass::CoroutineType
            | KnownClass::NotImplementedType
            | KnownClass::BuiltinFunctionType
            | KnownClass::EllipsisType
            | KnownClass::NoneType
            | KnownClass::Awaitable
            | KnownClass::Generator
            | KnownClass::Deprecated
            | KnownClass::StdlibAlias
            | KnownClass::SpecialForm
            | KnownClass::TypeVar
            | KnownClass::ExtensionsTypeVar
            | KnownClass::ParamSpec
            | KnownClass::ParamSpecArgs
            | KnownClass::ParamSpecKwargs
            | KnownClass::ProtocolMeta
            | KnownClass::TypeVarTuple
            | KnownClass::TypeAliasType
            | KnownClass::NoDefaultType
            | KnownClass::NewType
            | KnownClass::SupportsIndex
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::Mapping
            | KnownClass::ChainMap
            | KnownClass::Counter
            | KnownClass::DefaultDict
            | KnownClass::Deque
            | KnownClass::OrderedDict
            | KnownClass::VersionInfo
            | KnownClass::Field
            | KnownClass::KwOnly
            | KnownClass::NamedTupleLike
            | KnownClass::Template
            | KnownClass::Path
            | KnownClass::ConstraintSet
            | KnownClass::GenericContext
            | KnownClass::Specialization
            | KnownClass::InitVar => false,
            KnownClass::NamedTupleFallback | KnownClass::TypedDictFallback => true,
        }
    }

    pub(crate) fn name(self, db: &dyn Db) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Object => "object",
            Self::Bytes => "bytes",
            Self::Bytearray => "bytearray",
            Self::Tuple => "tuple",
            Self::Int => "int",
            Self::Float => "float",
            Self::Complex => "complex",
            Self::FrozenSet => "frozenset",
            Self::Str => "str",
            Self::Set => "set",
            Self::Dict => "dict",
            Self::List => "list",
            Self::Type => "type",
            Self::Slice => "slice",
            Self::Property => "property",
            Self::BaseException => "BaseException",
            Self::BaseExceptionGroup => "BaseExceptionGroup",
            Self::Exception => "Exception",
            Self::ExceptionGroup => "ExceptionGroup",
            Self::Staticmethod => "staticmethod",
            Self::Classmethod => "classmethod",
            Self::Awaitable => "Awaitable",
            Self::Generator => "Generator",
            Self::Deprecated => "deprecated",
            Self::GenericAlias => "GenericAlias",
            Self::ModuleType => "ModuleType",
            Self::FunctionType => "FunctionType",
            Self::MethodType => "MethodType",
            Self::UnionType => "UnionType",
            Self::MethodWrapperType => "MethodWrapperType",
            Self::WrapperDescriptorType => "WrapperDescriptorType",
            Self::BuiltinFunctionType => "BuiltinFunctionType",
            Self::GeneratorType => "GeneratorType",
            Self::AsyncGeneratorType => "AsyncGeneratorType",
            Self::CoroutineType => "CoroutineType",
            Self::NoneType => "NoneType",
            Self::SpecialForm => "_SpecialForm",
            Self::TypeVar => "TypeVar",
            Self::ExtensionsTypeVar => "TypeVar",
            Self::ParamSpec => "ParamSpec",
            Self::ParamSpecArgs => "ParamSpecArgs",
            Self::ParamSpecKwargs => "ParamSpecKwargs",
            Self::TypeVarTuple => "TypeVarTuple",
            Self::TypeAliasType => "TypeAliasType",
            Self::NoDefaultType => "_NoDefaultType",
            Self::NewType => "NewType",
            Self::SupportsIndex => "SupportsIndex",
            Self::ChainMap => "ChainMap",
            Self::Counter => "Counter",
            Self::DefaultDict => "defaultdict",
            Self::Deque => "deque",
            Self::OrderedDict => "OrderedDict",
            Self::Enum => "Enum",
            Self::EnumType => {
                if Program::get(db).python_version(db) >= PythonVersion::PY311 {
                    "EnumType"
                } else {
                    "EnumMeta"
                }
            }
            Self::Auto => "auto",
            Self::Member => "member",
            Self::Nonmember => "nonmember",
            Self::StrEnum => "StrEnum",
            Self::ABCMeta => "ABCMeta",
            Self::Super => "super",
            Self::Iterable => "Iterable",
            Self::Iterator => "Iterator",
            Self::Mapping => "Mapping",
            // For example, `typing.List` is defined as `List = _Alias()` in typeshed
            Self::StdlibAlias => "_Alias",
            // This is the name the type of `sys.version_info` has in typeshed,
            // which is different to what `type(sys.version_info).__name__` is at runtime.
            // (At runtime, `type(sys.version_info).__name__ == "version_info"`,
            // which is impossible to replicate in the stubs since the sole instance of the class
            // also has that name in the `sys` module.)
            Self::VersionInfo => "_version_info",
            Self::EllipsisType => {
                // Exposed as `types.EllipsisType` on Python >=3.10;
                // backported as `builtins.ellipsis` by typeshed on Python <=3.9
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    "EllipsisType"
                } else {
                    "ellipsis"
                }
            }
            Self::NotImplementedType => {
                // Exposed as `types.NotImplementedType` on Python >=3.10;
                // backported as `builtins._NotImplementedType` by typeshed on Python <=3.9
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    "NotImplementedType"
                } else {
                    "_NotImplementedType"
                }
            }
            Self::Field => "Field",
            Self::KwOnly => "KW_ONLY",
            Self::InitVar => "InitVar",
            Self::NamedTupleFallback => "NamedTupleFallback",
            Self::NamedTupleLike => "NamedTupleLike",
            Self::ConstraintSet => "ConstraintSet",
            Self::GenericContext => "GenericContext",
            Self::Specialization => "Specialization",
            Self::TypedDictFallback => "TypedDictFallback",
            Self::Template => "Template",
            Self::Path => "Path",
            Self::ProtocolMeta => "_ProtocolMeta",
        }
    }

    pub(super) fn display(self, db: &dyn Db) -> impl std::fmt::Display + '_ {
        struct KnownClassDisplay<'db> {
            db: &'db dyn Db,
            class: KnownClass,
        }

        impl std::fmt::Display for KnownClassDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let KnownClassDisplay {
                    class: known_class,
                    db,
                } = *self;
                write!(
                    f,
                    "{module}.{class}",
                    module = known_class.canonical_module(db),
                    class = known_class.name(db)
                )
            }
        }

        KnownClassDisplay { db, class: self }
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`] representing all possible instances of
    /// the class. If this class is generic, this will use the default specialization.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    #[track_caller]
    pub fn to_instance(self, db: &dyn Db) -> Type<'_> {
        debug_assert_ne!(
            self,
            KnownClass::Tuple,
            "Use `Type::heterogeneous_tuple` or `Type::homogeneous_tuple` to create `tuple` instances"
        );
        self.to_class_literal(db)
            .to_class_type(db)
            .map(|class| Type::instance(db, class))
            .unwrap_or_else(Type::unknown)
    }

    /// Similar to [`KnownClass::to_instance`], but returns the Unknown-specialization where each type
    /// parameter is specialized to `Unknown`.
    #[track_caller]
    pub(crate) fn to_instance_unknown(self, db: &dyn Db) -> Type<'_> {
        debug_assert_ne!(
            self,
            KnownClass::Tuple,
            "Use `Type::heterogeneous_tuple` or `Type::homogeneous_tuple` to create `tuple` instances"
        );
        self.try_to_class_literal(db)
            .map(|literal| Type::instance(db, literal.unknown_specialization(db)))
            .unwrap_or_else(Type::unknown)
    }

    /// Lookup a generic [`KnownClass`] in typeshed and return a [`Type`]
    /// representing a specialization of that class.
    ///
    /// If the class cannot be found in typeshed, or if you provide a specialization with the wrong
    /// number of types, a debug-level log message will be emitted stating this.
    pub(crate) fn to_specialized_class_type<'db>(
        self,
        db: &'db dyn Db,
        specialization: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<ClassType<'db>> {
        let Type::ClassLiteral(class_literal) = self.to_class_literal(db) else {
            return None;
        };
        let generic_context = class_literal.generic_context(db)?;

        let types = specialization.into_iter().collect::<Box<[_]>>();
        if types.len() != generic_context.len(db) {
            // a cache of the `KnownClass`es that we have already seen mismatched-arity
            // specializations for (and therefore that we've already logged a warning for)
            static MESSAGES: LazyLock<Mutex<FxHashSet<KnownClass>>> = LazyLock::new(Mutex::default);
            if MESSAGES.lock().unwrap().insert(self) {
                tracing::info!(
                    "Wrong number of types when specializing {}. \
                     Falling back to default specialization for the symbol instead.",
                    self.display(db)
                );
            }
            return Some(class_literal.default_specialization(db));
        }

        Some(class_literal.apply_specialization(db, |_| generic_context.specialize(db, types)))
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`]
    /// representing all possible instances of the generic class with a specialization.
    ///
    /// If the class cannot be found in typeshed, or if you provide a specialization with the wrong
    /// number of types, a debug-level log message will be emitted stating this.
    #[track_caller]
    pub(crate) fn to_specialized_instance<'db>(
        self,
        db: &'db dyn Db,
        specialization: impl IntoIterator<Item = Type<'db>>,
    ) -> Type<'db> {
        debug_assert_ne!(
            self,
            KnownClass::Tuple,
            "Use `Type::heterogeneous_tuple` or `Type::homogeneous_tuple` to create `tuple` instances"
        );
        self.to_specialized_class_type(db, specialization)
            .and_then(|class_type| Type::from(class_type).to_instance(db))
            .unwrap_or_else(Type::unknown)
    }

    /// Attempt to lookup a [`KnownClass`] in typeshed and return a [`Type`] representing that class-literal.
    ///
    /// Return an error if the symbol cannot be found in the expected typeshed module,
    /// or if the symbol is not a class definition, or if the symbol is possibly unbound.
    fn try_to_class_literal_without_logging(
        self,
        db: &dyn Db,
    ) -> Result<ClassLiteral<'_>, KnownClassLookupError<'_>> {
        let symbol = known_module_symbol(db, self.canonical_module(db), self.name(db)).place;
        match symbol {
            Place::Defined(Type::ClassLiteral(class_literal), _, Definedness::AlwaysDefined) => {
                Ok(class_literal)
            }
            Place::Defined(
                Type::ClassLiteral(class_literal),
                _,
                Definedness::PossiblyUndefined,
            ) => Err(KnownClassLookupError::ClassPossiblyUnbound { class_literal }),
            Place::Defined(found_type, _, _) => {
                Err(KnownClassLookupError::SymbolNotAClass { found_type })
            }
            Place::Undefined => Err(KnownClassLookupError::ClassNotFound),
        }
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`] representing that class-literal.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub(crate) fn try_to_class_literal(self, db: &dyn Db) -> Option<ClassLiteral<'_>> {
        // a cache of the `KnownClass`es that we have already failed to lookup in typeshed
        // (and therefore that we've already logged a warning for)
        static MESSAGES: LazyLock<Mutex<FxHashSet<KnownClass>>> = LazyLock::new(Mutex::default);

        self.try_to_class_literal_without_logging(db)
            .or_else(|lookup_error| {
                if MESSAGES.lock().unwrap().insert(self) {
                    if matches!(
                        lookup_error,
                        KnownClassLookupError::ClassPossiblyUnbound { .. }
                    ) {
                        tracing::info!("{}", lookup_error.display(db, self));
                    } else {
                        tracing::info!(
                            "{}. Falling back to `Unknown` for the symbol instead.",
                            lookup_error.display(db, self)
                        );
                    }
                }

                match lookup_error {
                    KnownClassLookupError::ClassPossiblyUnbound { class_literal, .. } => {
                        Ok(class_literal)
                    }
                    KnownClassLookupError::ClassNotFound { .. }
                    | KnownClassLookupError::SymbolNotAClass { .. } => Err(()),
                }
            })
            .ok()
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`] representing that class-literal.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub(crate) fn to_class_literal(self, db: &dyn Db) -> Type<'_> {
        self.try_to_class_literal(db)
            .map(Type::ClassLiteral)
            .unwrap_or_else(Type::unknown)
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`]
    /// representing that class and all possible subclasses of the class.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub fn to_subclass_of(self, db: &dyn Db) -> Type<'_> {
        self.to_class_literal(db)
            .to_class_type(db)
            .map(|class| SubclassOfType::from(db, class))
            .unwrap_or_else(SubclassOfType::subclass_of_unknown)
    }

    /// Return `true` if this symbol can be resolved to a class definition `class` in typeshed,
    /// *and* `class` is a subclass of `other`.
    pub(super) fn is_subclass_of<'db>(self, db: &'db dyn Db, other: ClassType<'db>) -> bool {
        self.try_to_class_literal_without_logging(db)
            .is_ok_and(|class| class.is_subclass_of(db, None, other))
    }

    pub(super) fn when_subclass_of<'db>(
        self,
        db: &'db dyn Db,
        other: ClassType<'db>,
    ) -> ConstraintSet<'db> {
        ConstraintSet::from(self.is_subclass_of(db, other))
    }

    /// Return the module in which we should look up the definition for this class
    pub(super) fn canonical_module(self, db: &dyn Db) -> KnownModule {
        match self {
            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
            | Self::Type
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::List
            | Self::Tuple
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Exception
            | Self::ExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Slice
            | Self::Super
            | Self::Property => KnownModule::Builtins,
            Self::VersionInfo => KnownModule::Sys,
            Self::ABCMeta => KnownModule::Abc,
            Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum => KnownModule::Enum,
            Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::CoroutineType
            | Self::MethodWrapperType
            | Self::UnionType
            | Self::BuiltinFunctionType
            | Self::WrapperDescriptorType => KnownModule::Types,
            Self::NoneType => KnownModule::Typeshed,
            Self::Awaitable
            | Self::Generator
            | Self::SpecialForm
            | Self::TypeVar
            | Self::StdlibAlias
            | Self::Iterable
            | Self::Iterator
            | Self::Mapping
            | Self::ProtocolMeta
            | Self::SupportsIndex => KnownModule::Typing,
            Self::TypeAliasType
            | Self::ExtensionsTypeVar
            | Self::TypeVarTuple
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::Deprecated
            | Self::NewType => KnownModule::TypingExtensions,
            Self::NoDefaultType => {
                let python_version = Program::get(db).python_version(db);

                // typing_extensions has a 3.13+ re-export for the `typing.NoDefault`
                // singleton, but not for `typing._NoDefaultType`. So we need to switch
                // to `typing._NoDefaultType` for newer versions:
                if python_version >= PythonVersion::PY313 {
                    KnownModule::Typing
                } else {
                    KnownModule::TypingExtensions
                }
            }
            Self::EllipsisType => {
                // Exposed as `types.EllipsisType` on Python >=3.10;
                // backported as `builtins.ellipsis` by typeshed on Python <=3.9
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    KnownModule::Types
                } else {
                    KnownModule::Builtins
                }
            }
            Self::NotImplementedType => {
                // Exposed as `types.NotImplementedType` on Python >=3.10;
                // backported as `builtins._NotImplementedType` by typeshed on Python <=3.9
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    KnownModule::Types
                } else {
                    KnownModule::Builtins
                }
            }
            Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict => KnownModule::Collections,
            Self::Field | Self::KwOnly | Self::InitVar => KnownModule::Dataclasses,
            Self::NamedTupleFallback | Self::TypedDictFallback => KnownModule::TypeCheckerInternals,
            Self::NamedTupleLike
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization => KnownModule::TyExtensions,
            Self::Template => KnownModule::Templatelib,
            Self::Path => KnownModule::Pathlib,
        }
    }

    /// Returns `Some(true)` if all instances of this `KnownClass` compare equal.
    /// Returns `None` for `KnownClass::Tuple`, since whether or not a tuple type
    /// is single-valued depends on the tuple spec.
    pub(super) const fn is_single_valued(self) -> Option<bool> {
        match self {
            Self::NoneType
            | Self::NoDefaultType
            | Self::VersionInfo
            | Self::EllipsisType
            | Self::TypeAliasType
            | Self::UnionType
            | Self::NotImplementedType => Some(true),

            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
            | Self::Type
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::List
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::Slice
            | Self::Property
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Exception
            | Self::ExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Awaitable
            | Self::Generator
            | Self::Deprecated
            | Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::CoroutineType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::WrapperDescriptorType
            | Self::SpecialForm
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::SupportsIndex
            | Self::StdlibAlias
            | Self::TypeVar
            | Self::ExtensionsTypeVar
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::ABCMeta
            | Self::Super
            | Self::NewType
            | Self::Field
            | Self::KwOnly
            | Self::InitVar
            | Self::Iterable
            | Self::Iterator
            | Self::Mapping
            | Self::NamedTupleFallback
            | Self::NamedTupleLike
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization
            | Self::TypedDictFallback
            | Self::BuiltinFunctionType
            | Self::ProtocolMeta
            | Self::Template
            | Self::Path => Some(false),

            Self::Tuple => None,
        }
    }

    /// Is this class a singleton class?
    ///
    /// A singleton class is a class where it is known that only one instance can ever exist at runtime.
    pub(super) const fn is_singleton(self) -> bool {
        match self {
            Self::NoneType
            | Self::EllipsisType
            | Self::NoDefaultType
            | Self::VersionInfo
            | Self::TypeAliasType
            | Self::NotImplementedType => true,

            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
            | Self::Tuple
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::List
            | Self::Type
            | Self::Slice
            | Self::Property
            | Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::WrapperDescriptorType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::CoroutineType
            | Self::SpecialForm
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::StdlibAlias
            | Self::SupportsIndex
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Exception
            | Self::ExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Awaitable
            | Self::Generator
            | Self::Deprecated
            | Self::TypeVar
            | Self::ExtensionsTypeVar
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::ABCMeta
            | Self::Super
            | Self::UnionType
            | Self::NewType
            | Self::Field
            | Self::KwOnly
            | Self::InitVar
            | Self::Iterable
            | Self::Iterator
            | Self::Mapping
            | Self::NamedTupleFallback
            | Self::NamedTupleLike
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization
            | Self::TypedDictFallback
            | Self::BuiltinFunctionType
            | Self::ProtocolMeta
            | Self::Template
            | Self::Path => false,
        }
    }

    pub(super) fn try_from_file_and_name(
        db: &dyn Db,
        file: File,
        class_name: &str,
    ) -> Option<Self> {
        // We assert that this match is exhaustive over the right-hand side in the unit test
        // `known_class_roundtrip_from_str()`
        let candidates: &[Self] = match class_name {
            "bool" => &[Self::Bool],
            "object" => &[Self::Object],
            "bytes" => &[Self::Bytes],
            "bytearray" => &[Self::Bytearray],
            "tuple" => &[Self::Tuple],
            "type" => &[Self::Type],
            "int" => &[Self::Int],
            "float" => &[Self::Float],
            "complex" => &[Self::Complex],
            "str" => &[Self::Str],
            "set" => &[Self::Set],
            "frozenset" => &[Self::FrozenSet],
            "dict" => &[Self::Dict],
            "list" => &[Self::List],
            "slice" => &[Self::Slice],
            "property" => &[Self::Property],
            "BaseException" => &[Self::BaseException],
            "BaseExceptionGroup" => &[Self::BaseExceptionGroup],
            "Exception" => &[Self::Exception],
            "ExceptionGroup" => &[Self::ExceptionGroup],
            "staticmethod" => &[Self::Staticmethod],
            "classmethod" => &[Self::Classmethod],
            "Awaitable" => &[Self::Awaitable],
            "Generator" => &[Self::Generator],
            "deprecated" => &[Self::Deprecated],
            "GenericAlias" => &[Self::GenericAlias],
            "NoneType" => &[Self::NoneType],
            "ModuleType" => &[Self::ModuleType],
            "GeneratorType" => &[Self::GeneratorType],
            "AsyncGeneratorType" => &[Self::AsyncGeneratorType],
            "CoroutineType" => &[Self::CoroutineType],
            "FunctionType" => &[Self::FunctionType],
            "MethodType" => &[Self::MethodType],
            "UnionType" => &[Self::UnionType],
            "MethodWrapperType" => &[Self::MethodWrapperType],
            "WrapperDescriptorType" => &[Self::WrapperDescriptorType],
            "BuiltinFunctionType" => &[Self::BuiltinFunctionType],
            "NewType" => &[Self::NewType],
            "TypeAliasType" => &[Self::TypeAliasType],
            "TypeVar" => &[Self::TypeVar, Self::ExtensionsTypeVar],
            "Iterable" => &[Self::Iterable],
            "Iterator" => &[Self::Iterator],
            "Mapping" => &[Self::Mapping],
            "ParamSpec" => &[Self::ParamSpec],
            "ParamSpecArgs" => &[Self::ParamSpecArgs],
            "ParamSpecKwargs" => &[Self::ParamSpecKwargs],
            "TypeVarTuple" => &[Self::TypeVarTuple],
            "ChainMap" => &[Self::ChainMap],
            "Counter" => &[Self::Counter],
            "defaultdict" => &[Self::DefaultDict],
            "deque" => &[Self::Deque],
            "OrderedDict" => &[Self::OrderedDict],
            "_Alias" => &[Self::StdlibAlias],
            "_SpecialForm" => &[Self::SpecialForm],
            "_NoDefaultType" => &[Self::NoDefaultType],
            "SupportsIndex" => &[Self::SupportsIndex],
            "Enum" => &[Self::Enum],
            "EnumMeta" => &[Self::EnumType],
            "EnumType" if Program::get(db).python_version(db) >= PythonVersion::PY311 => {
                &[Self::EnumType]
            }
            "StrEnum" if Program::get(db).python_version(db) >= PythonVersion::PY311 => {
                &[Self::StrEnum]
            }
            "auto" => &[Self::Auto],
            "member" => &[Self::Member],
            "nonmember" => &[Self::Nonmember],
            "ABCMeta" => &[Self::ABCMeta],
            "super" => &[Self::Super],
            "_version_info" => &[Self::VersionInfo],
            "ellipsis" if Program::get(db).python_version(db) <= PythonVersion::PY39 => {
                &[Self::EllipsisType]
            }
            "EllipsisType" if Program::get(db).python_version(db) >= PythonVersion::PY310 => {
                &[Self::EllipsisType]
            }
            "_NotImplementedType" if Program::get(db).python_version(db) <= PythonVersion::PY39 => {
                &[Self::NotImplementedType]
            }
            "NotImplementedType" if Program::get(db).python_version(db) >= PythonVersion::PY310 => {
                &[Self::NotImplementedType]
            }
            "Field" => &[Self::Field],
            "KW_ONLY" => &[Self::KwOnly],
            "InitVar" => &[Self::InitVar],
            "NamedTupleFallback" => &[Self::NamedTupleFallback],
            "NamedTupleLike" => &[Self::NamedTupleLike],
            "ConstraintSet" => &[Self::ConstraintSet],
            "GenericContext" => &[Self::GenericContext],
            "Specialization" => &[Self::Specialization],
            "TypedDictFallback" => &[Self::TypedDictFallback],
            "Template" => &[Self::Template],
            "Path" => &[Self::Path],
            "_ProtocolMeta" => &[Self::ProtocolMeta],
            _ => return None,
        };

        let module = file_to_module(db, file)?.known(db)?;

        candidates
            .iter()
            .copied()
            .find(|&candidate| candidate.check_module(db, module))
    }

    /// Return `true` if the module of `self` matches `module`
    fn check_module(self, db: &dyn Db, module: KnownModule) -> bool {
        match self {
            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
            | Self::Type
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::List
            | Self::Tuple
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::Slice
            | Self::Property
            | Self::GenericAlias
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::StdlibAlias  // no equivalent class exists in typing_extensions, nor ever will
            | Self::ModuleType
            | Self::VersionInfo
            | Self::BaseException
            | Self::Exception
            | Self::ExceptionGroup
            | Self::EllipsisType
            | Self::BaseExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::ABCMeta
            | Self::Super
            | Self::NotImplementedType
            | Self::UnionType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::CoroutineType
            | Self::WrapperDescriptorType
            | Self::BuiltinFunctionType
            | Self::Field
            | Self::KwOnly
            | Self::InitVar
            | Self::NamedTupleFallback
            | Self::TypedDictFallback
            | Self::TypeVar
            | Self::ExtensionsTypeVar
            | Self::NamedTupleLike
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization
            | Self::Awaitable
            | Self::Generator
            | Self::Template
            | Self::Path => module == self.canonical_module(db),
            Self::NoneType => matches!(module, KnownModule::Typeshed | KnownModule::Types),
            Self::SpecialForm
            | Self::TypeAliasType
            | Self::NoDefaultType
            | Self::SupportsIndex
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Iterable
            | Self::Iterator
            | Self::Mapping
            | Self::ProtocolMeta
            | Self::NewType => matches!(module, KnownModule::Typing | KnownModule::TypingExtensions),
            Self::Deprecated => matches!(module, KnownModule::Warnings | KnownModule::TypingExtensions),
        }
    }

    /// Evaluate a call to this known class, emit any diagnostics that are necessary
    /// as a result of the call, and return the type that results from the call.
    pub(super) fn check_call<'db>(
        self,
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
        overload: &mut Binding<'db>,
        call_expression: &ast::ExprCall,
    ) {
        let db = context.db();
        let scope = context.scope();
        let module = context.module();

        match self {
            KnownClass::Super => {
                // Handle the case where `super()` is called with no arguments.
                // In this case, we need to infer the two arguments:
                //   1. The nearest enclosing class
                //   2. The first parameter of the current function (typically `self` or `cls`)
                match overload.parameter_types() {
                    [] => {
                        let Some(enclosing_class) = nearest_enclosing_class(db, index, scope)
                        else {
                            BoundSuperError::UnavailableImplicitArguments
                                .report_diagnostic(context, call_expression.into());
                            overload.set_return_type(Type::unknown());
                            return;
                        };

                        // Check if the enclosing class is a `NamedTuple`, which forbids the use of `super()`.
                        if CodeGeneratorKind::NamedTuple.matches(db, enclosing_class, None) {
                            if let Some(builder) = context
                                .report_lint(&SUPER_CALL_IN_NAMED_TUPLE_METHOD, call_expression)
                            {
                                builder.into_diagnostic(format_args!(
                                    "Cannot use `super()` in a method of NamedTuple class `{}`",
                                    enclosing_class.name(db)
                                ));
                            }
                            overload.set_return_type(Type::unknown());
                            return;
                        }

                        // The type of the first parameter if the given scope is function-like (i.e. function or lambda).
                        // `None` if the scope is not function-like, or has no parameters.
                        let first_param = match scope.node(db) {
                            NodeWithScopeKind::Function(f) => {
                                f.node(module).parameters.iter().next()
                            }
                            NodeWithScopeKind::Lambda(l) => l
                                .node(module)
                                .parameters
                                .as_ref()
                                .into_iter()
                                .flatten()
                                .next(),
                            _ => None,
                        };

                        let Some(first_param) = first_param else {
                            BoundSuperError::UnavailableImplicitArguments
                                .report_diagnostic(context, call_expression.into());
                            overload.set_return_type(Type::unknown());
                            return;
                        };

                        let definition = index.expect_single_definition(first_param);
                        let first_param = binding_type(db, definition);

                        let bound_super = BoundSuperType::build(
                            db,
                            Type::ClassLiteral(enclosing_class),
                            first_param,
                        )
                        .unwrap_or_else(|err| {
                            err.report_diagnostic(context, call_expression.into());
                            Type::unknown()
                        });

                        overload.set_return_type(bound_super);
                    }
                    [Some(pivot_class_type), Some(owner_type)] => {
                        // Check if the enclosing class is a `NamedTuple`, which forbids the use of `super()`.
                        if let Some(enclosing_class) = nearest_enclosing_class(db, index, scope) {
                            if CodeGeneratorKind::NamedTuple.matches(db, enclosing_class, None) {
                                if let Some(builder) = context
                                    .report_lint(&SUPER_CALL_IN_NAMED_TUPLE_METHOD, call_expression)
                                {
                                    builder.into_diagnostic(format_args!(
                                        "Cannot use `super()` in a method of NamedTuple class `{}`",
                                        enclosing_class.name(db)
                                    ));
                                }
                                overload.set_return_type(Type::unknown());
                                return;
                            }
                        }

                        let bound_super = BoundSuperType::build(db, *pivot_class_type, *owner_type)
                            .unwrap_or_else(|err| {
                                err.report_diagnostic(context, call_expression.into());
                                Type::unknown()
                            });
                        overload.set_return_type(bound_super);
                    }
                    _ => {}
                }
            }

            KnownClass::Deprecated => {
                // Parsing something of the form:
                //
                // @deprecated("message")
                // @deprecated("message", caregory = DeprecationWarning, stacklevel = 1)
                //
                // "Static type checker behavior is not affected by the category and stacklevel arguments"
                // so we only need the message and can ignore everything else. The message is mandatory,
                // must be a LiteralString, and always comes first.
                //
                // We aren't guaranteed to know the static value of a LiteralString, so we need to
                // accept that sometimes we will fail to include the message.
                //
                // We don't do any serious validation/diagnostics here, as the signature for this
                // is included in `Type::bindings`.
                //
                // See: <https://typing.python.org/en/latest/spec/directives.html#deprecated>
                let [Some(message), ..] = overload.parameter_types() else {
                    // Checking in Type::bindings will complain about this for us
                    return;
                };

                overload.set_return_type(Type::KnownInstance(KnownInstanceType::Deprecated(
                    DeprecatedInstance::new(db, message.as_string_literal()),
                )));
            }

            KnownClass::TypeAliasType => {
                let assigned_to = index
                    .try_expression(ast::ExprRef::from(call_expression))
                    .and_then(|expr| expr.assigned_to(db));

                let containing_assignment = assigned_to.as_ref().and_then(|assigned_to| {
                    match assigned_to.node(module).targets.as_slice() {
                        [ast::Expr::Name(target)] => Some(index.expect_single_definition(target)),
                        _ => None,
                    }
                });

                let [Some(name), Some(value), ..] = overload.parameter_types() else {
                    return;
                };

                let Some(name) = name.as_string_literal() else {
                    if let Some(builder) =
                        context.report_lint(&INVALID_TYPE_ALIAS_TYPE, call_expression)
                    {
                        builder.into_diagnostic(
                            "The name of a `typing.TypeAlias` must be a string literal",
                        );
                    }
                    return;
                };
                overload.set_return_type(Type::KnownInstance(KnownInstanceType::TypeAliasType(
                    TypeAliasType::ManualPEP695(ManualPEP695TypeAliasType::new(
                        db,
                        ast::name::Name::new(name.value(db)),
                        containing_assignment,
                        value,
                    )),
                )));
            }

            _ => {}
        }
    }
}

/// Enumeration of ways in which looking up a [`KnownClass`] in typeshed could fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KnownClassLookupError<'db> {
    /// There is no symbol by that name in the expected typeshed module.
    ClassNotFound,
    /// There is a symbol by that name in the expected typeshed module,
    /// but it's not a class.
    SymbolNotAClass { found_type: Type<'db> },
    /// There is a symbol by that name in the expected typeshed module,
    /// and it's a class definition, but it's possibly unbound.
    ClassPossiblyUnbound { class_literal: ClassLiteral<'db> },
}

impl<'db> KnownClassLookupError<'db> {
    fn display(&self, db: &'db dyn Db, class: KnownClass) -> impl std::fmt::Display + 'db {
        struct ErrorDisplay<'db> {
            db: &'db dyn Db,
            class: KnownClass,
            error: KnownClassLookupError<'db>,
        }

        impl std::fmt::Display for ErrorDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let ErrorDisplay { db, class, error } = *self;

                let class = class.display(db);
                let python_version = Program::get(db).python_version(db);

                match error {
                    KnownClassLookupError::ClassNotFound => write!(
                        f,
                        "Could not find class `{class}` in typeshed on Python {python_version}",
                    ),
                    KnownClassLookupError::SymbolNotAClass { found_type } => write!(
                        f,
                        "Error looking up `{class}` in typeshed: expected to find a class definition \
                        on Python {python_version}, but found a symbol of type `{found_type}` instead",
                        found_type = found_type.display(db),
                    ),
                    KnownClassLookupError::ClassPossiblyUnbound { .. } => write!(
                        f,
                        "Error looking up `{class}` in typeshed on Python {python_version}: \
                        expected to find a fully bound symbol, but found one that is possibly unbound",
                    ),
                }
            }
        }

        ErrorDisplay {
            db,
            class,
            error: *self,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(super) struct MetaclassError<'db> {
    kind: MetaclassErrorKind<'db>,
}

impl<'db> MetaclassError<'db> {
    /// Return an [`MetaclassErrorKind`] variant describing why we could not resolve the metaclass for this class.
    pub(super) fn reason(&self) -> &MetaclassErrorKind<'db> {
        &self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(super) enum MetaclassErrorKind<'db> {
    /// The class has incompatible metaclasses in its inheritance hierarchy.
    ///
    /// The metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all
    /// its bases.
    Conflict {
        /// `candidate1` will either be the explicit `metaclass=` keyword in the class definition,
        /// or the inferred metaclass of a base class
        candidate1: MetaclassCandidate<'db>,

        /// `candidate2` will always be the inferred metaclass of a base class
        candidate2: MetaclassCandidate<'db>,

        /// Flag to indicate whether `candidate1` is the explicit `metaclass=` keyword or the
        /// inferred metaclass of a base class. This helps us give better error messages in diagnostics.
        candidate1_is_base_class: bool,
    },
    /// The metaclass is not callable
    NotCallable(Type<'db>),
    /// The metaclass is of a union type whose some members are not callable
    PartlyNotCallable(Type<'db>),
    /// A cycle was encountered attempting to determine the metaclass
    Cycle,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SlotsKind {
    /// `__slots__` is not found in the class.
    NotSpecified,
    /// `__slots__` is defined but empty: `__slots__ = ()`.
    Empty,
    /// `__slots__` is defined and is not empty: `__slots__ = ("a", "b")`.
    NotEmpty,
    /// `__slots__` is defined but its value is dynamic:
    /// * `__slots__ = tuple(a for a in b)`
    /// * `__slots__ = ["a", "b"]`
    Dynamic,
}

impl SlotsKind {
    fn from(db: &dyn Db, base: ClassLiteral) -> Self {
        let Place::Defined(slots_ty, _, bound) = base
            .own_class_member(db, base.inherited_generic_context(db), None, "__slots__")
            .inner
            .place
        else {
            return Self::NotSpecified;
        };

        if matches!(bound, Definedness::PossiblyUndefined) {
            return Self::Dynamic;
        }

        match slots_ty {
            // __slots__ = ("a", "b")
            Type::NominalInstance(nominal) => match nominal
                .tuple_spec(db)
                .and_then(|spec| spec.len().into_fixed_length())
            {
                Some(0) => Self::Empty,
                Some(_) => Self::NotEmpty,
                None => Self::Dynamic,
            },

            // __slots__ = "abc"  # Same as `("abc",)`
            Type::StringLiteral(_) => Self::NotEmpty,

            _ => Self::Dynamic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::setup_db;
    use crate::module_resolver::resolve_module_confident;
    use crate::{PythonVersionSource, PythonVersionWithSource};
    use salsa::Setter;
    use strum::IntoEnumIterator;

    #[test]
    fn known_class_roundtrip_from_str() {
        let mut db = setup_db();
        Program::get(&db)
            .set_python_version_with_source(&mut db)
            .to(PythonVersionWithSource {
                version: PythonVersion::latest_preview(),
                source: PythonVersionSource::default(),
            });
        for class in KnownClass::iter() {
            let class_name = class.name(&db);
            let class_module =
                resolve_module_confident(&db, &class.canonical_module(&db).name()).unwrap();

            assert_eq!(
                KnownClass::try_from_file_and_name(
                    &db,
                    class_module.file(&db).unwrap(),
                    class_name
                ),
                Some(class),
                "`KnownClass::candidate_from_str` appears to be missing a case for `{class_name}`"
            );
        }
    }

    #[test]
    fn known_class_doesnt_fallback_to_unknown_unexpectedly_on_latest_version() {
        let mut db = setup_db();

        Program::get(&db)
            .set_python_version_with_source(&mut db)
            .to(PythonVersionWithSource {
                version: PythonVersion::latest_ty(),
                source: PythonVersionSource::default(),
            });

        for class in KnownClass::iter() {
            // Check the class can be looked up successfully
            class.try_to_class_literal_without_logging(&db).unwrap();

            // We can't call `KnownClass::Tuple.to_instance()`;
            // there are assertions to ensure that we always call `Type::homogeneous_tuple()`
            // or `Type::heterogeneous_tuple()` instead.`
            if class != KnownClass::Tuple {
                assert_ne!(
                    class.to_instance(&db),
                    Type::unknown(),
                    "Unexpectedly fell back to `Unknown` for `{class:?}`"
                );
            }
        }
    }

    #[test]
    fn known_class_doesnt_fallback_to_unknown_unexpectedly_on_low_python_version() {
        let mut db = setup_db();

        // First, collect the `KnownClass` variants
        // and sort them according to the version they were added in.
        // This makes the test far faster as it minimizes the number of times
        // we need to change the Python version in the loop.
        let mut classes: Vec<(KnownClass, PythonVersion)> = KnownClass::iter()
            .map(|class| {
                let version_added = match class {
                    KnownClass::Template => PythonVersion::PY314,
                    KnownClass::UnionType => PythonVersion::PY310,
                    KnownClass::BaseExceptionGroup | KnownClass::ExceptionGroup => {
                        PythonVersion::PY311
                    }
                    KnownClass::GenericAlias => PythonVersion::PY39,
                    KnownClass::KwOnly => PythonVersion::PY310,
                    KnownClass::Member | KnownClass::Nonmember | KnownClass::StrEnum => {
                        PythonVersion::PY311
                    }
                    _ => PythonVersion::PY37,
                };
                (class, version_added)
            })
            .collect();

        classes.sort_unstable_by_key(|(_, version)| *version);

        let program = Program::get(&db);
        let mut current_version = program.python_version(&db);

        for (class, version_added) in classes {
            if version_added != current_version {
                program
                    .set_python_version_with_source(&mut db)
                    .to(PythonVersionWithSource {
                        version: version_added,
                        source: PythonVersionSource::default(),
                    });
                current_version = version_added;
            }

            // Check the class can be looked up successfully
            class.try_to_class_literal_without_logging(&db).unwrap();

            // We can't call `KnownClass::Tuple.to_instance()`;
            // there are assertions to ensure that we always call `Type::homogeneous_tuple()`
            // or `Type::heterogeneous_tuple()` instead.`
            if class != KnownClass::Tuple {
                assert_ne!(
                    class.to_instance(&db),
                    Type::unknown(),
                    "Unexpectedly fell back to `Unknown` for `{class:?}` on Python {version_added}"
                );
            }
        }
    }
}
