use std::hash::BuildHasherDefault;
use std::sync::{LazyLock, Mutex};

use super::TypeVarVariance;
use super::{
    IntersectionBuilder, MemberLookupPolicy, Mro, MroError, MroIterator, SpecialFormType,
    SubclassOfType, Truthiness, Type, TypeQualifiers,
    class_base::ClassBase,
    function::{FunctionDecorators, FunctionType},
    infer_expression_type, infer_unpack_types,
};
use crate::semantic_index::definition::{Definition, DefinitionState};
use crate::semantic_index::place::NodeWithScopeKind;
use crate::semantic_index::{DeclarationWithConstraint, SemanticIndex, attribute_declarations};
use crate::types::context::InferContext;
use crate::types::diagnostic::{INVALID_LEGACY_TYPE_VARIABLE, INVALID_TYPE_ALIAS_TYPE};
use crate::types::enums::enum_metadata;
use crate::types::function::{DataclassTransformerParams, KnownFunction};
use crate::types::generics::{GenericContext, Specialization, walk_specialization};
use crate::types::infer::nearest_enclosing_class;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::tuple::TupleType;
use crate::types::{
    BareTypeAliasType, Binding, BoundSuperError, BoundSuperType, CallableType, DataclassParams,
    DynamicType, KnownInstanceType, TypeAliasType, TypeMapping, TypeRelation, TypeTransformer,
    TypeVarBoundOrConstraints, TypeVarInstance, TypeVarKind, infer_definition_types,
};
use crate::{
    Db, FxOrderSet, KnownModule, Program,
    module_resolver::file_to_module,
    place::{
        Boundness, LookupError, LookupResult, Place, PlaceAndQualifiers, class_symbol,
        known_module_symbol, place_from_bindings, place_from_declarations,
    },
    semantic_index::{
        attribute_assignments,
        definition::{DefinitionKind, TargetKind},
        place::ScopeId,
        place_table, semantic_index, use_def_map,
    },
    types::{
        CallArguments, CallError, CallErrorKind, MetaclassCandidate, UnionBuilder, UnionType,
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
use rustc_hash::{FxHashSet, FxHasher};

type FxOrderMap<K, V> = ordermap::map::OrderMap<K, V, BuildHasherDefault<FxHasher>>;

fn explicit_bases_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &[Type<'db>],
    _count: u32,
    _self: ClassLiteral<'db>,
) -> salsa::CycleRecoveryAction<Box<[Type<'db>]>> {
    salsa::CycleRecoveryAction::Iterate
}

fn explicit_bases_cycle_initial<'db>(
    _db: &'db dyn Db,
    _self: ClassLiteral<'db>,
) -> Box<[Type<'db>]> {
    Box::default()
}

#[expect(clippy::ref_option, clippy::trivially_copy_pass_by_ref)]
fn inheritance_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Option<InheritanceCycle>,
    _count: u32,
    _self: ClassLiteral<'db>,
) -> salsa::CycleRecoveryAction<Option<InheritanceCycle>> {
    salsa::CycleRecoveryAction::Iterate
}

fn inheritance_cycle_initial<'db>(
    _db: &'db dyn Db,
    _self: ClassLiteral<'db>,
) -> Option<InheritanceCycle> {
    None
}

fn try_mro_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Result<Mro<'db>, MroError<'db>>,
    _count: u32,
    _self: ClassLiteral<'db>,
    _specialization: Option<Specialization<'db>>,
) -> salsa::CycleRecoveryAction<Result<Mro<'db>, MroError<'db>>> {
    salsa::CycleRecoveryAction::Iterate
}

fn try_mro_cycle_initial<'db>(
    db: &'db dyn Db,
    self_: ClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
) -> Result<Mro<'db>, MroError<'db>> {
    Err(MroError::cycle(
        db,
        self_.apply_optional_specialization(db, specialization),
    ))
}

fn try_metaclass_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Result<(Type<'db>, Option<DataclassTransformerParams>), MetaclassError<'db>>,
    _count: u32,
    _self: ClassLiteral<'db>,
) -> salsa::CycleRecoveryAction<
    Result<(Type<'db>, Option<DataclassTransformerParams>), MetaclassError<'db>>,
> {
    salsa::CycleRecoveryAction::Iterate
}

#[allow(clippy::unnecessary_wraps)]
fn try_metaclass_cycle_initial<'db>(
    _db: &'db dyn Db,
    _self_: ClassLiteral<'db>,
) -> Result<(Type<'db>, Option<DataclassTransformerParams>), MetaclassError<'db>> {
    Err(MetaclassError {
        kind: MetaclassErrorKind::Cycle,
    })
}

/// A category of classes with code generation capabilities (with synthesized methods).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CodeGeneratorKind {
    /// Classes decorated with `@dataclass` or similar dataclass-like decorators
    DataclassLike,
    /// Classes inheriting from `typing.NamedTuple`
    NamedTuple,
}

impl CodeGeneratorKind {
    pub(crate) fn from_class(db: &dyn Db, class: ClassLiteral<'_>) -> Option<Self> {
        if CodeGeneratorKind::DataclassLike.matches(db, class) {
            Some(CodeGeneratorKind::DataclassLike)
        } else if CodeGeneratorKind::NamedTuple.matches(db, class) {
            Some(CodeGeneratorKind::NamedTuple)
        } else {
            None
        }
    }

    fn matches<'db>(self, db: &'db dyn Db, class: ClassLiteral<'db>) -> bool {
        match self {
            Self::DataclassLike => {
                class.dataclass_params(db).is_some()
                    || class
                        .try_metaclass(db)
                        .is_ok_and(|(_, transformer_params)| transformer_params.is_some())
            }
            Self::NamedTuple => class.explicit_bases(db).iter().any(|base| {
                base.into_class_literal()
                    .is_some_and(|c| c.is_known(db, KnownClass::NamedTuple))
            }),
        }
    }
}

/// A specialization of a generic class with a particular assignment of types to typevars.
///
/// # Ordering
/// Ordering is based on the generic aliases's salsa-assigned id and not on its values.
/// The id may change between runs, or when the alias was garbage collected and recreated.
#[salsa::interned(debug)]
#[derive(PartialOrd, Ord)]
pub struct GenericAlias<'db> {
    pub(crate) origin: ClassLiteral<'db>,
    pub(crate) specialization: Specialization<'db>,
}

pub(super) fn walk_generic_alias<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    alias: GenericAlias<'db>,
    visitor: &mut V,
) {
    walk_specialization(db, alias.specialization(db), visitor);
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for GenericAlias<'_> {}

impl<'db> GenericAlias<'db> {
    pub(super) fn normalized_impl(
        self,
        db: &'db dyn Db,
        visitor: &mut TypeTransformer<'db>,
    ) -> Self {
        Self::new(
            db,
            self.origin(db),
            self.specialization(db).normalized_impl(db, visitor),
        )
    }

    pub(super) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self::new(
            db,
            self.origin(db),
            self.specialization(db).materialize(db, variance),
        )
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.origin(db).definition(db)
    }

    pub(super) fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        Self::new(
            db,
            self.origin(db),
            self.specialization(db).apply_type_mapping(db, type_mapping),
        )
    }

    pub(super) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        // A tuple's specialization will include all of its element types, so we don't need to also
        // look in `self.tuple`.
        self.specialization(db).find_legacy_typevars(db, typevars);
    }
}

impl<'db> From<GenericAlias<'db>> for Type<'db> {
    fn from(alias: GenericAlias<'db>) -> Type<'db> {
        Type::GenericAlias(alias)
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
    NonGeneric(ClassLiteral<'db>),
    Generic(GenericAlias<'db>),
}

#[salsa::tracked]
impl<'db> ClassType<'db> {
    pub(super) fn normalized_impl(
        self,
        db: &'db dyn Db,
        visitor: &mut TypeTransformer<'db>,
    ) -> Self {
        match self {
            Self::NonGeneric(_) => self,
            Self::Generic(generic) => Self::Generic(generic.normalized_impl(db, visitor)),
        }
    }

    pub(super) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        match self {
            Self::NonGeneric(_) => self,
            Self::Generic(generic) => Self::Generic(generic.materialize(db, variance)),
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

    pub(crate) fn known(self, db: &'db dyn Db) -> Option<KnownClass> {
        let (class_literal, _) = self.class_literal(db);
        class_literal.known(db)
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let (class_literal, _) = self.class_literal(db);
        class_literal.definition(db)
    }

    /// Return `Some` if this class is known to be a [`SolidBase`], or `None` if it is not.
    pub(super) fn as_solid_base(self, db: &'db dyn Db) -> Option<SolidBase<'db>> {
        self.class_literal(db).0.as_solid_base(db)
    }

    /// Return `true` if this class represents `known_class`
    pub(crate) fn is_known(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.known(db) == Some(known_class)
    }

    /// Return `true` if this class represents the builtin class `object`
    pub(crate) fn is_object(self, db: &'db dyn Db) -> bool {
        self.is_known(db, KnownClass::Object)
    }

    pub(super) fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        match self {
            Self::NonGeneric(_) => self,
            Self::Generic(generic) => Self::Generic(generic.apply_type_mapping(db, type_mapping)),
        }
    }

    pub(super) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        match self {
            Self::NonGeneric(_) => {}
            Self::Generic(generic) => generic.find_legacy_typevars(db, typevars),
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
        self.has_relation_to(db, other, TypeRelation::Subtyping)
    }

    pub(super) fn has_relation_to(
        self,
        db: &'db dyn Db,
        other: Self,
        relation: TypeRelation,
    ) -> bool {
        // TODO: remove this branch once we have proper support for TypedDicts.
        if self.is_known(db, KnownClass::Dict)
            && other
                .iter_mro(db)
                .any(|b| matches!(b, ClassBase::Dynamic(DynamicType::TodoTypedDict)))
        {
            return true;
        }

        self.iter_mro(db).any(|base| {
            match base {
                ClassBase::Dynamic(_) => match relation {
                    TypeRelation::Subtyping => other.is_object(db),
                    TypeRelation::Assignability => !other.is_final(db),
                },

                // Protocol and Generic are not represented by a ClassType.
                ClassBase::Protocol | ClassBase::Generic => false,

                ClassBase::Class(base) => match (base, other) {
                    (ClassType::NonGeneric(base), ClassType::NonGeneric(other)) => base == other,
                    (ClassType::Generic(base), ClassType::Generic(other)) => {
                        base.origin(db) == other.origin(db)
                            && base.specialization(db).has_relation_to(
                                db,
                                other.specialization(db),
                                relation,
                            )
                    }
                    (ClassType::Generic(_), ClassType::NonGeneric(_))
                    | (ClassType::NonGeneric(_), ClassType::Generic(_)) => false,
                },
            }
        })
    }

    pub(super) fn is_equivalent_to(self, db: &'db dyn Db, other: ClassType<'db>) -> bool {
        if self == other {
            return true;
        }

        match (self, other) {
            // A non-generic class is never equivalent to a generic class.
            // Two non-generic classes are only equivalent if they are equal (handled above).
            (ClassType::NonGeneric(_), _) | (_, ClassType::NonGeneric(_)) => false,

            (ClassType::Generic(this), ClassType::Generic(other)) => {
                this.origin(db) == other.origin(db)
                    && this
                        .specialization(db)
                        .is_equivalent_to(db, other.specialization(db))
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

    /// Return the [`SolidBase`] that appears first in the MRO of this class.
    ///
    /// Returns `None` if this class does not have any solid bases in its MRO.
    pub(super) fn nearest_solid_base(self, db: &'db dyn Db) -> Option<SolidBase<'db>> {
        self.iter_mro(db)
            .filter_map(ClassBase::into_class)
            .find_map(|base| base.as_solid_base(db))
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

        // Optimisation: if either class is `@final`, we only need to do one `is_subclass_of` call.
        if self.is_final(db) {
            return self.is_subclass_of(db, other);
        }
        if other.is_final(db) {
            return other.is_subclass_of(db, self);
        }

        // Two solid bases can only coexist in an MRO if one is a subclass of the other.
        if self.nearest_solid_base(db).is_some_and(|solid_base_1| {
            other.nearest_solid_base(db).is_some_and(|solid_base_2| {
                !solid_base_1.could_coexist_in_mro_with(db, &solid_base_2)
            })
        }) {
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
    /// or those marked as ClassVars are considered.
    ///
    /// Returns [`Place::Unbound`] if `name` cannot be found in this class's scope
    /// directly. Use [`ClassType::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal
            .own_class_member(db, specialization, name)
            .map_type(|ty| ty.apply_optional_specialization(db, specialization))
    }

    /// Look up an instance attribute (available in `__dict__`) of the given name.
    ///
    /// See [`Type::instance_member`] for more details.
    pub(super) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal
            .instance_member(db, specialization, name)
            .map_type(|ty| ty.apply_optional_specialization(db, specialization))
    }

    /// A helper function for `instance_member` that looks up the `name` attribute only on
    /// this class, not on its superclasses.
    fn own_instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal
            .own_instance_member(db, name)
            .map_type(|ty| ty.apply_optional_specialization(db, specialization))
    }

    /// Return a callable type (or union of callable types) that represents the callable
    /// constructor signature of this class.
    pub(super) fn into_callable(self, db: &'db dyn Db) -> Type<'db> {
        let self_ty = Type::from(self);
        let metaclass_dunder_call_function_symbol = self_ty
            .member_lookup_with_policy(
                db,
                "__call__".into(),
                MemberLookupPolicy::NO_INSTANCE_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            )
            .place;

        if let Place::Type(Type::BoundMethod(metaclass_dunder_call_function), _) =
            metaclass_dunder_call_function_symbol
        {
            // TODO: this intentionally diverges from step 1 in
            // https://typing.python.org/en/latest/spec/constructors.html#converting-a-constructor-to-callable
            // by always respecting the signature of the metaclass `__call__`, rather than
            // using a heuristic which makes unwarranted assumptions to sometimes ignore it.
            return Type::Callable(metaclass_dunder_call_function.into_callable_type(db));
        }

        let dunder_new_function_symbol = self_ty
            .member_lookup_with_policy(
                db,
                "__new__".into(),
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
            )
            .place;

        let dunder_new_function =
            if let Place::Type(Type::FunctionLiteral(dunder_new_function), _) =
                dunder_new_function_symbol
            {
                // Step 3: If the return type of the `__new__` evaluates to a type that is not a subclass of this class,
                // then we should ignore the `__init__` and just return the `__new__` method.
                let returns_non_subclass =
                    dunder_new_function
                        .signature(db)
                        .overloads
                        .iter()
                        .any(|signature| {
                            signature.return_ty.is_some_and(|return_ty| {
                                !return_ty.is_assignable_to(
                                    db,
                                    self_ty
                                        .to_instance(db)
                                        .expect("ClassType should be instantiable"),
                                )
                            })
                        });

                let dunder_new_bound_method =
                    dunder_new_function.into_bound_method_type(db, self_ty);

                if returns_non_subclass {
                    return dunder_new_bound_method;
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
        let synthesized_dunder_init_callable =
            if let Place::Type(ty, _) = dunder_init_function_symbol {
                let signature = match ty {
                    Type::FunctionLiteral(dunder_init_function) => {
                        Some(dunder_init_function.signature(db))
                    }
                    Type::Callable(callable) => Some(callable.signatures(db)),
                    _ => None,
                };

                if let Some(signature) = signature {
                    let synthesized_signature = |signature: &Signature<'db>| {
                        Signature::new(signature.parameters().clone(), Some(correct_return_type))
                            .with_definition(signature.definition())
                            .bind_self()
                    };

                    let synthesized_dunder_init_signature = CallableSignature::from_overloads(
                        signature.overloads.iter().map(synthesized_signature),
                    );

                    Some(Type::Callable(CallableType::new(
                        db,
                        synthesized_dunder_init_signature,
                        true,
                    )))
                } else {
                    None
                }
            } else {
                None
            };

        match (dunder_new_function, synthesized_dunder_init_callable) {
            (Some(dunder_new_function), Some(synthesized_dunder_init_callable)) => {
                UnionType::from_elements(
                    db,
                    vec![dunder_new_function, synthesized_dunder_init_callable],
                )
            }
            (Some(constructor), None) | (None, Some(constructor)) => constructor,
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

                if let Place::Type(Type::FunctionLiteral(new_function), _) = new_function_symbol {
                    new_function.into_bound_method_type(db, self_ty)
                } else {
                    // Fallback if no `object.__new__` is found.
                    CallableType::single(
                        db,
                        Signature::new(Parameters::empty(), Some(correct_return_type)),
                    )
                }
            }
        }
    }
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

/// A filter that describes which methods are considered when looking for implicit attribute assignments
/// in [`ClassLiteral::implicit_attribute`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum MethodDecorator {
    None,
    ClassMethod,
    StaticMethod,
}

impl MethodDecorator {
    fn try_from_fn_type(db: &dyn Db, fn_type: FunctionType) -> Result<Self, ()> {
        match (
            fn_type.has_known_decorator(db, FunctionDecorators::CLASSMETHOD),
            fn_type.has_known_decorator(db, FunctionDecorators::STATICMETHOD),
        ) {
            (true, true) => Err(()), // A method can't be static and class method at the same time.
            (true, false) => Ok(Self::ClassMethod),
            (false, true) => Ok(Self::StaticMethod),
            (false, false) => Ok(Self::None),
        }
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
#[salsa::interned(debug)]
#[derive(PartialOrd, Ord)]
pub struct ClassLiteral<'db> {
    /// Name of the class at definition
    #[returns(ref)]
    pub(crate) name: ast::name::Name,

    pub(crate) body_scope: ScopeId<'db>,

    pub(crate) known: Option<KnownClass>,

    pub(crate) dataclass_params: Option<DataclassParams>,
    pub(crate) dataclass_transformer_params: Option<DataclassTransformerParams>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ClassLiteral<'_> {}

#[expect(clippy::trivially_copy_pass_by_ref, clippy::ref_option)]
fn pep695_generic_context_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Option<GenericContext<'db>>,
    _count: u32,
    _self: ClassLiteral<'db>,
) -> salsa::CycleRecoveryAction<Option<GenericContext<'db>>> {
    salsa::CycleRecoveryAction::Iterate
}

fn pep695_generic_context_cycle_initial<'db>(
    _db: &'db dyn Db,
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

    #[salsa::tracked(cycle_fn=pep695_generic_context_cycle_recover, cycle_initial=pep695_generic_context_cycle_initial, heap_size=get_size2::GetSize::get_heap_size)]
    pub(crate) fn pep695_generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        let scope = self.body_scope(db);
        let parsed = parsed_module(db, scope.file(db)).load(db);
        let class_def_node = scope.node(db).expect_class(&parsed);
        class_def_node.type_params.as_ref().map(|type_params| {
            let index = semantic_index(db, scope.file(db));
            GenericContext::from_type_params(db, index, type_params)
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

    pub(crate) fn inherited_legacy_generic_context(
        self,
        db: &'db dyn Db,
    ) -> Option<GenericContext<'db>> {
        GenericContext::from_base_classes(
            db,
            self.explicit_bases(db)
                .iter()
                .copied()
                .filter(|ty| matches!(ty, Type::GenericAlias(_))),
        )
    }

    fn file(self, db: &dyn Db) -> File {
        self.body_scope(db).file(db)
    }

    /// Return the original [`ast::StmtClassDef`] node associated with this class
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn node<'ast>(self, db: &'db dyn Db, module: &'ast ParsedModuleRef) -> &'ast ast::StmtClassDef {
        let scope = self.body_scope(db);
        scope.node(db).expect_class(module)
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let body_scope = self.body_scope(db);
        let module = parsed_module(db, body_scope.file(db)).load(db);
        let index = semantic_index(db, body_scope.file(db));
        index.expect_single_definition(body_scope.node(db).expect_class(&module))
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
            specialization.unwrap_or_else(|| generic_context.default_specialization(db))
        })
    }

    /// Returns the default specialization of this class. For non-generic classes, the class is
    /// returned unchanged. For a non-specialized generic class, we return a generic alias that
    /// applies the default specialization to the class's typevars.
    pub(crate) fn default_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            generic_context.default_specialization(db)
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
    #[salsa::tracked(returns(deref), cycle_fn=explicit_bases_cycle_recover, cycle_initial=explicit_bases_cycle_initial, heap_size=get_size2::GetSize::get_heap_size)]
    pub(super) fn explicit_bases(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        tracing::trace!("ClassLiteral::explicit_bases_query: {}", self.name(db));

        let module = parsed_module(db, self.file(db)).load(db);
        let class_stmt = self.node(db, &module);
        let class_definition =
            semantic_index(db, self.file(db)).expect_single_definition(class_stmt);

        class_stmt
            .bases()
            .iter()
            .map(|base_node| definition_expression_type(db, class_definition, base_node))
            .collect()
    }

    /// Return `Some()` if this class is known to be a [`SolidBase`], or `None` if it is not.
    pub(super) fn as_solid_base(self, db: &'db dyn Db) -> Option<SolidBase<'db>> {
        if let Some(known_class) = self.known(db) {
            known_class
                .is_solid_base()
                .then_some(SolidBase::hard_coded(self))
        } else if SlotsKind::from(db, self) == SlotsKind::NotEmpty {
            Some(SolidBase::due_to_dunder_slots(self))
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
            .into_class_literal()
            .is_some_and(|metaclass| metaclass.is_known(db, KnownClass::ABCMeta))
    }

    /// Return the types of the decorators on this class
    #[salsa::tracked(returns(deref), heap_size=get_size2::GetSize::get_heap_size)]
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
            .filter_map(|deco| deco.into_function_literal())
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
    #[salsa::tracked(returns(as_ref), cycle_fn=try_mro_cycle_recover, cycle_initial=try_mro_cycle_initial, heap_size=get_size2::GetSize::get_heap_size)]
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
    #[salsa::tracked(
        cycle_fn=try_metaclass_cycle_recover,
        cycle_initial=try_metaclass_cycle_initial,
        heap_size=get_size2::GetSize::get_heap_size,
    )]
    pub(super) fn try_metaclass(
        self,
        db: &'db dyn Db,
    ) -> Result<(Type<'db>, Option<DataclassTransformerParams>), MetaclassError<'db>> {
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
            let bases = TupleType::from_elements(db, self.explicit_bases(db).iter().copied());
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
        self.class_member_inner(db, None, name, policy)
    }

    fn class_member_inner(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        if name == "__mro__" {
            let tuple_elements = self.iter_mro(db, specialization).map(Type::from);
            return Place::bound(TupleType::from_elements(db, tuple_elements)).into();
        }

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
            Err(LookupError::Unbound(TypeQualifiers::empty()));

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
                    if class.is_known(db, KnownClass::Object)
                        // Only exclude `object` members if this is not an `object` class itself
                        && (policy.mro_no_object_fallback() && !self.is_known(db, KnownClass::Object))
                    {
                        continue;
                    }

                    if class.is_known(db, KnownClass::Type) && policy.meta_class_no_type_fallback()
                    {
                        continue;
                    }

                    lookup_result = lookup_result.or_else(|lookup_error| {
                        lookup_error.or_fall_back_to(db, class.own_class_member(db, name))
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
                    place: Place::Type(ty, _),
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
                    place: Place::Unbound,
                    qualifiers,
                },
                Some(dynamic_type),
            ) => Place::bound(dynamic_type).with_qualifiers(qualifiers),
        }
    }

    /// Returns the inferred type of the class member named `name`. Only bound members
    /// or those marked as ClassVars are considered.
    ///
    /// Returns [`Place::Unbound`] if `name` cannot be found in this class's scope
    /// directly. Use [`ClassLiteral::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(super) fn own_class_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        if name == "__dataclass_fields__" && self.dataclass_params(db).is_some() {
            // Make this class look like a subclass of the `DataClassInstance` protocol
            return Place::bound(KnownClass::Dict.to_specialized_instance(
                db,
                [
                    KnownClass::Str.to_instance(db),
                    KnownClass::Field.to_specialized_instance(db, [Type::any()]),
                ],
            ))
            .with_qualifiers(TypeQualifiers::CLASS_VAR);
        }

        let body_scope = self.body_scope(db);
        let symbol = class_symbol(db, body_scope, name).map_type(|ty| {
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
            match (self.generic_context(db), ty, specialization, name) {
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

        if symbol.place.is_unbound() {
            if let Some(synthesized_member) = self.own_synthesized_member(db, specialization, name)
            {
                return Place::bound(synthesized_member).into();
            }
            // The symbol was not found in the class scope. It might still be implicitly defined in `@classmethod`s.
            return Self::implicit_attribute(db, body_scope, name, MethodDecorator::ClassMethod)
                .into();
        }
        symbol
    }

    /// Returns the type of a synthesized dataclass member like `__init__` or `__lt__`, or
    /// a synthesized `__new__` method for a `NamedTuple`.
    fn own_synthesized_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Option<Type<'db>> {
        let dataclass_params = self.dataclass_params(db);
        let has_dataclass_param =
            |param| dataclass_params.is_some_and(|params| params.contains(param));

        let field_policy = CodeGeneratorKind::from_class(db, self)?;

        let signature_from_fields = |mut parameters: Vec<_>| {
            let mut kw_only_field_seen = false;
            for (name, (mut attr_ty, mut default_ty)) in
                self.fields(db, specialization, field_policy)
            {
                if attr_ty
                    .into_nominal_instance()
                    .is_some_and(|instance| instance.class.is_known(db, KnownClass::KwOnly))
                {
                    // Attributes annotated with `dataclass.KW_ONLY` are not present in the synthesized
                    // `__init__` method; they are used to indicate that the following parameters are
                    // keyword-only.
                    kw_only_field_seen = true;
                    continue;
                }

                let dunder_set = attr_ty.class_member(db, "__set__".into());
                if let Place::Type(dunder_set, Boundness::Bound) = dunder_set.place {
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
                        attr_ty = value_types.build();

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

                let mut parameter = if kw_only_field_seen {
                    Parameter::keyword_only(name)
                } else {
                    Parameter::positional_or_keyword(name)
                }
                .with_annotated_type(attr_ty);

                if let Some(default_ty) = default_ty {
                    parameter = parameter.with_default_type(default_ty);
                }

                parameters.push(parameter);
            }

            let mut signature = Signature::new(Parameters::new(parameters), Some(Type::none(db)));
            signature.inherited_generic_context = self.generic_context(db);
            Some(CallableType::function_like(db, signature))
        };

        match (field_policy, name) {
            (CodeGeneratorKind::DataclassLike, "__init__") => {
                let has_synthesized_dunder_init = has_dataclass_param(DataclassParams::INIT)
                    || self
                        .try_metaclass(db)
                        .is_ok_and(|(_, transformer_params)| transformer_params.is_some());

                if !has_synthesized_dunder_init {
                    return None;
                }

                let self_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                    // TODO: could be `Self`.
                    .with_annotated_type(Type::instance(
                        db,
                        self.apply_optional_specialization(db, specialization),
                    ));
                signature_from_fields(vec![self_parameter])
            }
            (CodeGeneratorKind::NamedTuple, "__new__") => {
                let cls_parameter = Parameter::positional_or_keyword(Name::new_static("cls"))
                    .with_annotated_type(KnownClass::Type.to_instance(db));
                signature_from_fields(vec![cls_parameter])
            }
            (CodeGeneratorKind::DataclassLike, "__lt__" | "__le__" | "__gt__" | "__ge__") => {
                if !has_dataclass_param(DataclassParams::ORDER) {
                    return None;
                }

                let signature = Signature::new(
                    Parameters::new([
                        Parameter::positional_or_keyword(Name::new_static("self"))
                            // TODO: could be `Self`.
                            .with_annotated_type(Type::instance(
                                db,
                                self.apply_optional_specialization(db, specialization),
                            )),
                        Parameter::positional_or_keyword(Name::new_static("other"))
                            // TODO: could be `Self`.
                            .with_annotated_type(Type::instance(
                                db,
                                self.apply_optional_specialization(db, specialization),
                            )),
                    ]),
                    Some(KnownClass::Bool.to_instance(db)),
                );

                Some(CallableType::function_like(db, signature))
            }
            (CodeGeneratorKind::NamedTuple, name) if name != "__init__" => {
                KnownClass::NamedTupleFallback
                    .to_class_literal(db)
                    .into_class_literal()?
                    .own_class_member(db, None, name)
                    .place
                    .ignore_possibly_unbound()
            }
            _ => None,
        }
    }

    /// Returns a list of all annotated attributes defined in this class, or any of its superclasses.
    ///
    /// See [`ClassLiteral::own_fields`] for more details.
    pub(crate) fn fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind,
    ) -> FxOrderMap<Name, (Type<'db>, Option<Type<'db>>)> {
        if field_policy == CodeGeneratorKind::NamedTuple {
            // NamedTuples do not allow multiple inheritance, so it is sufficient to enumerate the
            // fields of this class only.
            return self.own_fields(db);
        }

        let matching_classes_in_mro: Vec<_> = self
            .iter_mro(db, specialization)
            .filter_map(|superclass| {
                if let Some(class) = superclass.into_class() {
                    let class_literal = class.class_literal(db).0;
                    if field_policy.matches(db, class_literal) {
                        Some(class_literal)
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
            .flat_map(|class| class.own_fields(db))
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
    fn own_fields(self, db: &'db dyn Db) -> FxOrderMap<Name, (Type<'db>, Option<Type<'db>>)> {
        let mut attributes = FxOrderMap::default();

        let class_body_scope = self.body_scope(db);
        let table = place_table(db, class_body_scope);

        let use_def = use_def_map(db, class_body_scope);
        for (place_id, declarations) in use_def.all_end_of_scope_declarations() {
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

            let place_expr = table.place_expr(place_id);

            if let Ok(attr) = place_from_declarations(db, declarations) {
                if attr.is_class_var() {
                    continue;
                }

                if let Some(attr_ty) = attr.place.ignore_possibly_unbound() {
                    let bindings = use_def.end_of_scope_bindings(place_id);
                    let default_ty = place_from_bindings(db, bindings).ignore_possibly_unbound();

                    attributes.insert(place_expr.expect_name().clone(), (attr_ty, default_ty));
                }
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
        let mut union = UnionBuilder::new(db);
        let mut union_qualifiers = TypeQualifiers::empty();

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
                        place: Place::Type(ty, boundness),
                        qualifiers,
                    } = class.own_instance_member(db, name)
                    {
                        // TODO: We could raise a diagnostic here if there are conflicting type qualifiers
                        union_qualifiers |= qualifiers;

                        if boundness == Boundness::Bound {
                            if union.is_empty() {
                                // Short-circuit, no need to allocate inside the union builder
                                return member;
                            }

                            return Place::bound(union.add(ty).build())
                                .with_qualifiers(union_qualifiers);
                        }

                        // If we see a possibly-unbound symbol, we need to keep looking
                        // higher up in the MRO.
                        union = union.add(ty);
                    }
                }
            }
        }

        if union.is_empty() {
            Place::Unbound.with_qualifiers(TypeQualifiers::empty())
        } else {
            // If we have reached this point, we know that we have only seen possibly-unbound places.
            // This means that the final result is still possibly-unbound.

            Place::Type(union.build(), Boundness::PossiblyUnbound).with_qualifiers(union_qualifiers)
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
    ) -> Place<'db> {
        // If we do not see any declarations of an attribute, neither in the class body nor in
        // any method, we build a union of `Unknown` with the inferred types of all bindings of
        // that attribute. We include `Unknown` in that union to account for the fact that the
        // attribute might be externally modified.
        let mut union_of_inferred_types = UnionBuilder::new(db).add(Type::unknown());

        let mut is_attribute_bound = false;

        let file = class_body_scope.file(db);
        let module = parsed_module(db, file).load(db);
        let index = semantic_index(db, file);
        let class_map = use_def_map(db, class_body_scope);
        let class_table = place_table(db, class_body_scope);

        let is_valid_scope = |method_scope: ScopeId<'db>| {
            if let Some(method_def) = method_scope.node(db).as_function(&module) {
                let method_name = method_def.name.as_str();
                if let Place::Type(Type::FunctionLiteral(method_type), _) =
                    class_symbol(db, class_body_scope, method_name).place
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
            attribute_declarations(db, class_body_scope, name)
        {
            let method_scope = method_scope_id.to_scope_id(db, file);
            if !is_valid_scope(method_scope) {
                continue;
            }

            for attribute_declaration in attribute_declarations {
                let DefinitionState::Defined(decl) = attribute_declaration.declaration else {
                    continue;
                };

                let DefinitionKind::AnnotatedAssignment(annotated) = decl.kind(db) else {
                    continue;
                };

                if use_def_map(db, method_scope)
                    .is_declaration_reachable(db, &attribute_declaration)
                    .is_always_false()
                {
                    continue;
                }

                let annotation_ty =
                    infer_expression_type(db, index.expression(annotated.annotation(&module)));

                return Place::bound(annotation_ty);
            }
        }

        for (attribute_assignments, method_scope_id) in
            attribute_assignments(db, class_body_scope, name)
        {
            let method_scope = method_scope_id.to_scope_id(db, file);
            if !is_valid_scope(method_scope) {
                continue;
            }

            let method_map = use_def_map(db, method_scope);

            // The attribute assignment inherits the reachability of the method which contains it
            let is_method_reachable =
                if let Some(method_def) = method_scope.node(db).as_function(&module) {
                    let method = index.expect_single_definition(method_def);
                    let method_place = class_table.place_id_by_name(&method_def.name).unwrap();
                    class_map
                        .all_reachable_bindings(method_place)
                        .find_map(|bind| {
                            (bind.binding.is_defined_and(|def| def == method))
                                .then(|| class_map.is_binding_reachable(db, &bind))
                        })
                        .unwrap_or(Truthiness::AlwaysFalse)
                } else {
                    Truthiness::AlwaysFalse
                };
            if is_method_reachable.is_always_false() {
                continue;
            }

            // Storage for the implicit `DefinitionState::Undefined` binding. If present, it
            // will be the first binding in the `attribute_assignments` iterator.
            let mut unbound_binding = None;

            for attribute_assignment in attribute_assignments {
                if let DefinitionState::Undefined = attribute_assignment.binding {
                    // Store the implicit unbound binding here so that we can delay the
                    // computation of `unbound_reachability` to the point when we actually
                    // need it. This is an optimization for the common case where the
                    // `unbound` binding is the only binding of the `name` attribute,
                    // i.e. if there is no `self.name = …` assignment in this method.
                    unbound_binding = Some(attribute_assignment);
                    continue;
                }

                let DefinitionState::Defined(binding) = attribute_assignment.binding else {
                    continue;
                };
                match method_map
                    .is_binding_reachable(db, &attribute_assignment)
                    .and(is_method_reachable)
                {
                    Truthiness::AlwaysTrue | Truthiness::Ambiguous => {
                        is_attribute_bound = true;
                    }
                    Truthiness::AlwaysFalse => {
                        continue;
                    }
                }

                // There is at least one attribute assignment that may be reachable, so if `unbound_reachability` is
                // always false then this attribute is considered bound.
                // TODO: this is incomplete logic since the attributes bound after termination are considered reachable.
                let unbound_reachability = unbound_binding
                    .as_ref()
                    .map(|binding| method_map.is_binding_reachable(db, binding))
                    .unwrap_or(Truthiness::AlwaysFalse);

                if unbound_reachability
                    .negate()
                    .and(is_method_reachable)
                    .is_always_true()
                {
                    is_attribute_bound = true;
                }

                match binding.kind(db) {
                    DefinitionKind::AnnotatedAssignment(ann_assign) => {
                        // We found an annotated assignment of one of the following forms (using 'self' in these
                        // examples, but we support arbitrary names for the first parameters of methods):
                        //
                        //     self.name: <annotation>
                        //     self.name: <annotation> = …

                        let annotation_ty = infer_expression_type(
                            db,
                            index.expression(ann_assign.annotation(&module)),
                        );

                        // TODO: check if there are conflicting declarations
                        if is_attribute_bound {
                            return Place::bound(annotation_ty);
                        }
                        unreachable!(
                            "If the attribute assignments are all invisible, inference of their types should be skipped"
                        );
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
                                );
                                // TODO: Potential diagnostics resulting from the iterable are currently not reported.
                                let inferred_ty = iterable_ty.iterate(db);

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
                                );
                                let inferred_ty = context_ty.enter(db);

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
                                );
                                // TODO: Potential diagnostics resulting from the iterable are currently not reported.
                                let inferred_ty = iterable_ty.iterate(db);

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

        if is_attribute_bound {
            Place::bound(union_of_inferred_types.build())
        } else {
            Place::Unbound
        }
    }

    /// A helper function for `instance_member` that looks up the `name` attribute only on
    /// this class, not on its superclasses.
    pub(crate) fn own_instance_member(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        // TODO: There are many things that are not yet implemented here:
        // - `typing.Final`
        // - Proper diagnostics

        let body_scope = self.body_scope(db);
        let table = place_table(db, body_scope);

        if let Some(place_id) = table.place_id_by_name(name) {
            let use_def = use_def_map(db, body_scope);

            let declarations = use_def.end_of_scope_declarations(place_id);
            let declared_and_qualifiers = place_from_declarations(db, declarations);

            match declared_and_qualifiers {
                Ok(PlaceAndQualifiers {
                    place: mut declared @ Place::Type(declared_ty, declaredness),
                    qualifiers,
                }) => {
                    // For the purpose of finding instance attributes, ignore `ClassVar`
                    // declarations:
                    if qualifiers.contains(TypeQualifiers::CLASS_VAR) {
                        declared = Place::Unbound;
                    }

                    // The attribute is declared in the class body.

                    let bindings = use_def.end_of_scope_bindings(place_id);
                    let inferred = place_from_bindings(db, bindings);
                    let has_binding = !inferred.is_unbound();

                    if has_binding {
                        // The attribute is declared and bound in the class body.

                        if let Some(implicit_ty) =
                            Self::implicit_attribute(db, body_scope, name, MethodDecorator::None)
                                .ignore_possibly_unbound()
                        {
                            if declaredness == Boundness::Bound {
                                // If a symbol is definitely declared, and we see
                                // attribute assignments in methods of the class,
                                // we trust the declared type.
                                declared.with_qualifiers(qualifiers)
                            } else {
                                Place::Type(
                                    UnionType::from_elements(db, [declared_ty, implicit_ty]),
                                    declaredness,
                                )
                                .with_qualifiers(qualifiers)
                            }
                        } else {
                            // The symbol is declared and bound in the class body,
                            // but we did not find any attribute assignments in
                            // methods of the class. This means that the attribute
                            // has a class-level default value, but it would not be
                            // found in a `__dict__` lookup.

                            Place::Unbound.into()
                        }
                    } else {
                        // The attribute is declared but not bound in the class body.
                        // We take this as a sign that this is intended to be a pure
                        // instance attribute, and we trust the declared type, unless
                        // it is possibly-undeclared. In the latter case, we also
                        // union with the inferred type from attribute assignments.

                        if declaredness == Boundness::Bound {
                            declared.with_qualifiers(qualifiers)
                        } else {
                            if let Some(implicit_ty) = Self::implicit_attribute(
                                db,
                                body_scope,
                                name,
                                MethodDecorator::None,
                            )
                            .ignore_possibly_unbound()
                            {
                                Place::Type(
                                    UnionType::from_elements(db, [declared_ty, implicit_ty]),
                                    declaredness,
                                )
                                .with_qualifiers(qualifiers)
                            } else {
                                declared.with_qualifiers(qualifiers)
                            }
                        }
                    }
                }

                Ok(PlaceAndQualifiers {
                    place: Place::Unbound,
                    qualifiers: _,
                }) => {
                    // The attribute is not *declared* in the class body. It could still be declared/bound
                    // in a method.

                    Self::implicit_attribute(db, body_scope, name, MethodDecorator::None).into()
                }
                Err((declared, _conflicting_declarations)) => {
                    // There are conflicting declarations for this attribute in the class body.
                    Place::bound(declared.inner_type()).with_qualifiers(declared.qualifiers())
                }
            }
        } else {
            // This attribute is neither declared nor bound in the class body.
            // It could still be implicitly defined in a method.

            Self::implicit_attribute(db, body_scope, name, MethodDecorator::None).into()
        }
    }

    pub(super) fn to_non_generic_instance(self, db: &'db dyn Db) -> Type<'db> {
        Type::instance(db, ClassType::NonGeneric(self))
    }

    /// Return this class' involvement in an inheritance cycle, if any.
    ///
    /// A class definition like this will fail at runtime,
    /// but we must be resilient to it or we could panic.
    #[salsa::tracked(cycle_fn=inheritance_cycle_recover, cycle_initial=inheritance_cycle_initial, heap_size=get_size2::GetSize::get_heap_size)]
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
        let class_node = class_scope.node(db).expect_class(&module);
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
}

impl<'db> From<ClassLiteral<'db>> for Type<'db> {
    fn from(class: ClassLiteral<'db>) -> Type<'db> {
        Type::ClassLiteral(class)
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
/// attribute dictionary. A "solid base" can be a class defined in a C extension which defines C-level
/// instance slots, or a Python class that defines non-empty `__slots__`.
///
/// Two solid bases can only coexist in a class's MRO if one is a subclass of the other. Knowing if
/// a class is "solid base" or not is therefore valuable for inferring whether two instance types or
/// two subclass-of types are disjoint from each other. It also allows us to detect possible
/// `TypeError`s resulting from class definitions.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub(super) struct SolidBase<'db> {
    pub(super) class: ClassLiteral<'db>,
    pub(super) kind: SolidBaseKind,
}

impl<'db> SolidBase<'db> {
    /// Creates a [`SolidBase`] instance where we know the class is a solid base
    /// because it is special-cased by ty.
    fn hard_coded(class: ClassLiteral<'db>) -> Self {
        Self {
            class,
            kind: SolidBaseKind::HardCoded,
        }
    }

    /// Creates a [`SolidBase`] instance where we know the class is a solid base
    /// because of its `__slots__` definition.
    fn due_to_dunder_slots(class: ClassLiteral<'db>) -> Self {
        Self {
            class,
            kind: SolidBaseKind::DefinesSlots,
        }
    }

    /// Two solid bases can only coexist in a class's MRO if one is a subclass of the other
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
pub(super) enum SolidBaseKind {
    /// We know the class is a solid base because of some hardcoded knowledge in ty.
    HardCoded,
    /// We know the class is a solid base because it has a non-empty `__slots__` definition.
    DefinesSlots,
}

/// Non-exhaustive enumeration of known classes (e.g. `builtins.int`, `typing.Any`, ...) to allow
/// for easier syntax when interacting with very common classes.
///
/// Feel free to expand this enum if you ever find yourself using the same class in multiple
/// places.
/// Note: good candidates are any classes in `[crate::module_resolver::module::KnownModule]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    Auto,
    Member,
    Nonmember,
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
    // Typeshed
    NoneType, // Part of `types` for Python >= 3.10
    // Typing
    Any,
    StdlibAlias,
    SpecialForm,
    TypeVar,
    ParamSpec,
    ParamSpecArgs,
    ParamSpecKwargs,
    TypeVarTuple,
    TypeAliasType,
    NoDefaultType,
    NamedTuple,
    NewType,
    SupportsIndex,
    Iterable,
    // Collections
    ChainMap,
    Counter,
    DefaultDict,
    Deque,
    OrderedDict,
    // sys
    VersionInfo,
    // Exposed as `types.EllipsisType` on Python >=3.10;
    // backported as `builtins.ellipsis` by typeshed on Python <=3.9
    EllipsisType,
    NotImplementedType,
    // dataclasses
    Field,
    KwOnly,
    // _typeshed._type_checker_internals
    NamedTupleFallback,
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
    pub(crate) const fn bool(self) -> Truthiness {
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
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Super
            | Self::WrapperDescriptorType
            | Self::UnionType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::MethodWrapperType => Truthiness::AlwaysTrue,

            Self::NoneType => Truthiness::AlwaysFalse,

            Self::Any
            | Self::BaseException
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
            | Self::Tuple
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
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::ABCMeta
            | Self::Iterable
            // Empty tuples are AlwaysFalse; non-empty tuples are AlwaysTrue
            | Self::NamedTuple
            // Evaluating `NotImplementedType` in a boolean context was deprecated in Python 3.9
            // and raises a `TypeError` in Python >=3.14
            // (see https://docs.python.org/3/library/constants.html#NotImplemented)
            | Self::NotImplementedType
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Field
            | Self::KwOnly
            | Self::NamedTupleFallback => Truthiness::Ambiguous,
        }
    }

    /// Return `true` if this class is a [`SolidBase`]
    const fn is_solid_base(self) -> bool {
        match self {
            Self::Object => false,

            // Most non-`@final` builtins (other than `object`) are solid bases.
            Self::Set
            | Self::FrozenSet
            | Self::BaseException
            | Self::Bytearray
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::List
            | Self::Tuple
            | Self::Dict
            | Self::Slice
            | Self::Property
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Type
            | Self::ModuleType
            | Self::Super
            | Self::GenericAlias
            | Self::Deque
            | Self::Bytes => true,

            // It doesn't really make sense to ask the question for `@final` types,
            // since these are "more than solid bases". But we'll anyway infer a `@final`
            // class as being disjoint from a class that doesn't appear in its MRO,
            // and we'll anyway complain if we see a class definition that includes a
            // `@final` class in its bases. We therefore return `false` here to avoid
            // unnecessary duplicate diagnostics elsewhere.
            Self::TypeVarTuple
            | Self::TypeAliasType
            | Self::UnionType
            | Self::NoDefaultType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::FunctionType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::StdlibAlias
            | Self::SpecialForm
            | Self::TypeVar
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::WrapperDescriptorType
            | Self::EllipsisType
            | Self::NotImplementedType
            | Self::KwOnly
            | Self::VersionInfo
            | Self::Bool
            | Self::NoneType => false,

            // Anything with a *runtime* MRO (N.B. sometimes different from the MRO that typeshed gives!)
            // with length >2, or anything that is implemented in pure Python, is not a solid base.
            Self::ABCMeta
            | Self::Any
            | Self::Enum
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::ChainMap
            | Self::Exception
            | Self::ExceptionGroup
            | Self::Field
            | Self::SupportsIndex
            | Self::NamedTuple
            | Self::NamedTupleFallback
            | Self::Counter
            | Self::DefaultDict
            | Self::OrderedDict
            | Self::NewType
            | Self::Iterable
            | Self::BaseExceptionGroup => false,
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
            | KnownClass::Super
            | KnownClass::Enum
            | KnownClass::Auto
            | KnownClass::Member
            | KnownClass::Nonmember
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
            | KnownClass::NoneType
            | KnownClass::Any
            | KnownClass::StdlibAlias
            | KnownClass::SpecialForm
            | KnownClass::TypeVar
            | KnownClass::ParamSpec
            | KnownClass::ParamSpecArgs
            | KnownClass::ParamSpecKwargs
            | KnownClass::TypeVarTuple
            | KnownClass::TypeAliasType
            | KnownClass::NoDefaultType
            | KnownClass::NamedTuple
            | KnownClass::NewType
            | KnownClass::SupportsIndex
            | KnownClass::Iterable
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
            | KnownClass::NamedTupleFallback => false,
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
            Self::SupportsIndex | Self::Iterable => true,

            Self::Any
            | Self::Bool
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
            | Self::GenericAlias
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::WrapperDescriptorType
            | Self::NoneType
            | Self::SpecialForm
            | Self::TypeVar
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::TypeAliasType
            | Self::NoDefaultType
            | Self::NamedTuple
            | Self::NewType
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::Enum
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::ABCMeta
            | Self::Super
            | Self::StdlibAlias
            | Self::VersionInfo
            | Self::EllipsisType
            | Self::NotImplementedType
            | Self::UnionType
            | Self::Field
            | Self::KwOnly
            | Self::NamedTupleFallback => false,
        }
    }

    pub(crate) fn name(self, db: &dyn Db) -> &'static str {
        match self {
            Self::Any => "Any",
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
            Self::GenericAlias => "GenericAlias",
            Self::ModuleType => "ModuleType",
            Self::FunctionType => "FunctionType",
            Self::MethodType => "MethodType",
            Self::UnionType => "UnionType",
            Self::MethodWrapperType => "MethodWrapperType",
            Self::WrapperDescriptorType => "WrapperDescriptorType",
            Self::GeneratorType => "GeneratorType",
            Self::AsyncGeneratorType => "AsyncGeneratorType",
            Self::NamedTuple => "NamedTuple",
            Self::NoneType => "NoneType",
            Self::SpecialForm => "_SpecialForm",
            Self::TypeVar => "TypeVar",
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
            Self::Auto => "auto",
            Self::Member => "member",
            Self::Nonmember => "nonmember",
            Self::ABCMeta => "ABCMeta",
            Self::Super => "super",
            Self::Iterable => "Iterable",
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
            Self::NotImplementedType => "_NotImplementedType",
            Self::Field => "Field",
            Self::KwOnly => "KW_ONLY",
            Self::NamedTupleFallback => "NamedTupleFallback",
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

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`]
    /// representing all possible instances of the class.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub(crate) fn to_instance(self, db: &dyn Db) -> Type {
        self.to_class_literal(db)
            .to_class_type(db)
            .map(|class| Type::instance(db, class))
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
    pub(crate) fn to_specialized_instance<'db>(
        self,
        db: &'db dyn Db,
        specialization: impl IntoIterator<Item = Type<'db>>,
    ) -> Type<'db> {
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
    ) -> Result<ClassLiteral, KnownClassLookupError> {
        let symbol = known_module_symbol(db, self.canonical_module(db), self.name(db)).place;
        match symbol {
            Place::Type(Type::ClassLiteral(class_literal), Boundness::Bound) => Ok(class_literal),
            Place::Type(Type::ClassLiteral(class_literal), Boundness::PossiblyUnbound) => {
                Err(KnownClassLookupError::ClassPossiblyUnbound { class_literal })
            }
            Place::Type(found_type, _) => {
                Err(KnownClassLookupError::SymbolNotAClass { found_type })
            }
            Place::Unbound => Err(KnownClassLookupError::ClassNotFound),
        }
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`] representing that class-literal.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub(crate) fn try_to_class_literal(self, db: &dyn Db) -> Option<ClassLiteral> {
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
    pub(crate) fn to_class_literal(self, db: &dyn Db) -> Type {
        self.try_to_class_literal(db)
            .map(Type::ClassLiteral)
            .unwrap_or_else(Type::unknown)
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`]
    /// representing that class and all possible subclasses of the class.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub(crate) fn to_subclass_of(self, db: &dyn Db) -> Type {
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

    /// Return the module in which we should look up the definition for this class
    fn canonical_module(self, db: &dyn Db) -> KnownModule {
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
            Self::Enum | Self::Auto | Self::Member | Self::Nonmember => KnownModule::Enum,
            Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::MethodWrapperType
            | Self::UnionType
            | Self::WrapperDescriptorType => KnownModule::Types,
            Self::NoneType => KnownModule::Typeshed,
            Self::Any
            | Self::SpecialForm
            | Self::TypeVar
            | Self::NamedTuple
            | Self::StdlibAlias
            | Self::Iterable
            | Self::SupportsIndex => KnownModule::Typing,
            Self::TypeAliasType
            | Self::TypeVarTuple
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
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
            Self::NotImplementedType => KnownModule::Builtins,
            Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict => KnownModule::Collections,
            Self::Field => KnownModule::Dataclasses,
            Self::KwOnly => KnownModule::Dataclasses,
            Self::NamedTupleFallback => KnownModule::TypeCheckerInternals,
        }
    }

    /// Return true if all instances of this `KnownClass` compare equal.
    pub(super) const fn is_single_valued(self) -> bool {
        match self {
            Self::NoneType
            | Self::NoDefaultType
            | Self::VersionInfo
            | Self::EllipsisType
            | Self::TypeAliasType
            | Self::UnionType
            | Self::NotImplementedType => true,

            Self::Any
            | Self::Bool
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
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Exception
            | Self::ExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
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
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Enum
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::ABCMeta
            | Self::Super
            | Self::NamedTuple
            | Self::NewType
            | Self::Field
            | Self::KwOnly
            | Self::Iterable
            | Self::NamedTupleFallback => false,
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

            Self::Any
            | Self::Bool
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
            | Self::TypeVar
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Enum
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::ABCMeta
            | Self::Super
            | Self::UnionType
            | Self::NamedTuple
            | Self::NewType
            | Self::Field
            | Self::KwOnly
            | Self::Iterable
            | Self::NamedTupleFallback => false,
        }
    }

    pub(super) fn try_from_file_and_name(
        db: &dyn Db,
        file: File,
        class_name: &str,
    ) -> Option<Self> {
        // We assert that this match is exhaustive over the right-hand side in the unit test
        // `known_class_roundtrip_from_str()`
        let candidate = match class_name {
            "Any" => Self::Any,
            "bool" => Self::Bool,
            "object" => Self::Object,
            "bytes" => Self::Bytes,
            "bytearray" => Self::Bytearray,
            "tuple" => Self::Tuple,
            "type" => Self::Type,
            "int" => Self::Int,
            "float" => Self::Float,
            "complex" => Self::Complex,
            "str" => Self::Str,
            "set" => Self::Set,
            "frozenset" => Self::FrozenSet,
            "dict" => Self::Dict,
            "list" => Self::List,
            "slice" => Self::Slice,
            "property" => Self::Property,
            "BaseException" => Self::BaseException,
            "BaseExceptionGroup" => Self::BaseExceptionGroup,
            "Exception" => Self::Exception,
            "ExceptionGroup" => Self::ExceptionGroup,
            "staticmethod" => Self::Staticmethod,
            "classmethod" => Self::Classmethod,
            "GenericAlias" => Self::GenericAlias,
            "NoneType" => Self::NoneType,
            "ModuleType" => Self::ModuleType,
            "GeneratorType" => Self::GeneratorType,
            "AsyncGeneratorType" => Self::AsyncGeneratorType,
            "FunctionType" => Self::FunctionType,
            "MethodType" => Self::MethodType,
            "UnionType" => Self::UnionType,
            "MethodWrapperType" => Self::MethodWrapperType,
            "WrapperDescriptorType" => Self::WrapperDescriptorType,
            "NamedTuple" => Self::NamedTuple,
            "NewType" => Self::NewType,
            "TypeAliasType" => Self::TypeAliasType,
            "TypeVar" => Self::TypeVar,
            "Iterable" => Self::Iterable,
            "ParamSpec" => Self::ParamSpec,
            "ParamSpecArgs" => Self::ParamSpecArgs,
            "ParamSpecKwargs" => Self::ParamSpecKwargs,
            "TypeVarTuple" => Self::TypeVarTuple,
            "ChainMap" => Self::ChainMap,
            "Counter" => Self::Counter,
            "defaultdict" => Self::DefaultDict,
            "deque" => Self::Deque,
            "OrderedDict" => Self::OrderedDict,
            "_Alias" => Self::StdlibAlias,
            "_SpecialForm" => Self::SpecialForm,
            "_NoDefaultType" => Self::NoDefaultType,
            "SupportsIndex" => Self::SupportsIndex,
            "Enum" => Self::Enum,
            "auto" => Self::Auto,
            "member" => Self::Member,
            "nonmember" => Self::Nonmember,
            "ABCMeta" => Self::ABCMeta,
            "super" => Self::Super,
            "_version_info" => Self::VersionInfo,
            "ellipsis" if Program::get(db).python_version(db) <= PythonVersion::PY39 => {
                Self::EllipsisType
            }
            "EllipsisType" if Program::get(db).python_version(db) >= PythonVersion::PY310 => {
                Self::EllipsisType
            }
            "_NotImplementedType" => Self::NotImplementedType,
            "Field" => Self::Field,
            "KW_ONLY" => Self::KwOnly,
            "NamedTupleFallback" => Self::NamedTupleFallback,
            _ => return None,
        };

        candidate
            .check_module(db, file_to_module(db, file)?.known()?)
            .then_some(candidate)
    }

    /// Return `true` if the module of `self` matches `module`
    fn check_module(self, db: &dyn Db, module: KnownModule) -> bool {
        match self {
            Self::Any
            | Self::Bool
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
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::ABCMeta
            | Self::Super
            | Self::NotImplementedType
            | Self::UnionType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::WrapperDescriptorType
            | Self::Field
            | Self::KwOnly
            | Self::NamedTupleFallback => module == self.canonical_module(db),
            Self::NoneType => matches!(module, KnownModule::Typeshed | KnownModule::Types),
            Self::SpecialForm
            | Self::TypeVar
            | Self::TypeAliasType
            | Self::NoDefaultType
            | Self::SupportsIndex
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::NamedTuple
            | Self::Iterable
            | Self::NewType => matches!(module, KnownModule::Typing | KnownModule::TypingExtensions),
        }
    }

    /// Evaluate a call to this known class, emit any diagnostics that are necessary
    /// as a result of the call, and return the type that results from the call.
    pub(super) fn check_call<'db>(
        self,
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
        overload_binding: &Binding<'db>,
        call_argument_types: &CallArguments<'_, 'db>,
        call_expression: &ast::ExprCall,
    ) -> Option<Type<'db>> {
        let db = context.db();
        let scope = context.scope();
        let module = context.module();

        match self {
            KnownClass::Super => {
                // Handle the case where `super()` is called with no arguments.
                // In this case, we need to infer the two arguments:
                //   1. The nearest enclosing class
                //   2. The first parameter of the current function (typically `self` or `cls`)
                match overload_binding.parameter_types() {
                    [] => {
                        let Some(enclosing_class) =
                            nearest_enclosing_class(db, index, scope, module)
                        else {
                            BoundSuperError::UnavailableImplicitArguments
                                .report_diagnostic(context, call_expression.into());
                            return Some(Type::unknown());
                        };

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
                            return Some(Type::unknown());
                        };

                        let definition = index.expect_single_definition(first_param);
                        let first_param =
                            infer_definition_types(db, definition).binding_type(definition);

                        let bound_super = BoundSuperType::build(
                            db,
                            Type::ClassLiteral(enclosing_class),
                            first_param,
                        )
                        .unwrap_or_else(|err| {
                            err.report_diagnostic(context, call_expression.into());
                            Type::unknown()
                        });

                        Some(bound_super)
                    }
                    [Some(pivot_class_type), Some(owner_type)] => {
                        let bound_super = BoundSuperType::build(db, *pivot_class_type, *owner_type)
                            .unwrap_or_else(|err| {
                                err.report_diagnostic(context, call_expression.into());
                                Type::unknown()
                            });

                        Some(bound_super)
                    }
                    _ => None,
                }
            }

            KnownClass::TypeVar => {
                let assigned_to = index
                    .try_expression(ast::ExprRef::from(call_expression))
                    .and_then(|expr| expr.assigned_to(db));

                let Some(target) = assigned_to.as_ref().and_then(|assigned_to| {
                    match assigned_to.node(module).targets.as_slice() {
                        [ast::Expr::Name(target)] => Some(target),
                        _ => None,
                    }
                }) else {
                    let builder =
                        context.report_lint(&INVALID_LEGACY_TYPE_VARIABLE, call_expression)?;
                    builder.into_diagnostic(
                        "A legacy `typing.TypeVar` must be immediately assigned to a variable",
                    );
                    return None;
                };

                let [
                    Some(name_param),
                    constraints,
                    bound,
                    default,
                    contravariant,
                    covariant,
                    _infer_variance,
                ] = overload_binding.parameter_types()
                else {
                    return None;
                };

                let covariant = covariant
                    .map(|ty| ty.bool(db))
                    .unwrap_or(Truthiness::AlwaysFalse);

                let contravariant = contravariant
                    .map(|ty| ty.bool(db))
                    .unwrap_or(Truthiness::AlwaysFalse);

                let variance = match (contravariant, covariant) {
                    (Truthiness::Ambiguous, _) => {
                        let builder =
                            context.report_lint(&INVALID_LEGACY_TYPE_VARIABLE, call_expression)?;
                        builder.into_diagnostic(
                            "The `contravariant` parameter of a legacy `typing.TypeVar` \
                                cannot have an ambiguous value",
                        );
                        return None;
                    }
                    (_, Truthiness::Ambiguous) => {
                        let builder =
                            context.report_lint(&INVALID_LEGACY_TYPE_VARIABLE, call_expression)?;
                        builder.into_diagnostic(
                            "The `covariant` parameter of a legacy `typing.TypeVar` \
                                cannot have an ambiguous value",
                        );
                        return None;
                    }
                    (Truthiness::AlwaysTrue, Truthiness::AlwaysTrue) => {
                        let builder =
                            context.report_lint(&INVALID_LEGACY_TYPE_VARIABLE, call_expression)?;
                        builder.into_diagnostic(
                            "A legacy `typing.TypeVar` cannot be both covariant and contravariant",
                        );
                        return None;
                    }
                    (Truthiness::AlwaysTrue, Truthiness::AlwaysFalse) => {
                        TypeVarVariance::Contravariant
                    }
                    (Truthiness::AlwaysFalse, Truthiness::AlwaysTrue) => TypeVarVariance::Covariant,
                    (Truthiness::AlwaysFalse, Truthiness::AlwaysFalse) => {
                        TypeVarVariance::Invariant
                    }
                };

                let name_param = name_param.into_string_literal().map(|name| name.value(db));

                if name_param.is_none_or(|name_param| name_param != target.id) {
                    let builder =
                        context.report_lint(&INVALID_LEGACY_TYPE_VARIABLE, call_expression)?;
                    builder.into_diagnostic(format_args!(
                        "The name of a legacy `typing.TypeVar`{} must match \
                            the name of the variable it is assigned to (`{}`)",
                        if let Some(name_param) = name_param {
                            format!(" (`{name_param}`)")
                        } else {
                            String::new()
                        },
                        target.id,
                    ));
                    return None;
                }

                let bound_or_constraint = match (bound, constraints) {
                    (Some(bound), None) => Some(TypeVarBoundOrConstraints::UpperBound(*bound)),

                    (None, Some(_constraints)) => {
                        // We don't use UnionType::from_elements or UnionBuilder here,
                        // because we don't want to simplify the list of constraints like
                        // we do with the elements of an actual union type.
                        // TODO: Consider using a new `OneOfType` connective here instead,
                        // since that more accurately represents the actual semantics of
                        // typevar constraints.
                        let elements = UnionType::new(
                            db,
                            overload_binding
                                .arguments_for_parameter(call_argument_types, 1)
                                .map(|(_, ty)| ty)
                                .collect::<Box<_>>(),
                        );
                        Some(TypeVarBoundOrConstraints::Constraints(elements))
                    }

                    // TODO: Emit a diagnostic that TypeVar cannot be both bounded and
                    // constrained
                    (Some(_), Some(_)) => return None,

                    (None, None) => None,
                };

                let containing_assignment = index.expect_single_definition(target);
                Some(Type::KnownInstance(KnownInstanceType::TypeVar(
                    TypeVarInstance::new(
                        db,
                        &target.id,
                        Some(containing_assignment),
                        bound_or_constraint,
                        variance,
                        *default,
                        TypeVarKind::Legacy,
                    ),
                )))
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

                let [Some(name), Some(value), ..] = overload_binding.parameter_types() else {
                    return None;
                };

                name.into_string_literal()
                    .map(|name| {
                        Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::Bare(
                            BareTypeAliasType::new(
                                db,
                                ast::name::Name::new(name.value(db)),
                                containing_assignment,
                                value,
                            ),
                        )))
                    })
                    .or_else(|| {
                        let builder =
                            context.report_lint(&INVALID_TYPE_ALIAS_TYPE, call_expression)?;
                        builder.into_diagnostic(
                            "The name of a `typing.TypeAlias` must be a string literal",
                        );
                        None
                    })
            }

            _ => None,
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

pub(crate) struct SliceLiteral {
    pub(crate) start: Option<i32>,
    pub(crate) stop: Option<i32>,
    pub(crate) step: Option<i32>,
}

impl<'db> Type<'db> {
    /// If this type represents a valid slice literal, returns a [`SliceLiteral`] describing it.
    /// Otherwise returns `None`.
    ///
    /// The type must be a specialization of the `slice` builtin type, where the specialized
    /// typevars are statically known integers or `None`.
    pub(crate) fn slice_literal(self, db: &'db dyn Db) -> Option<SliceLiteral> {
        let ClassType::Generic(alias) = self.into_nominal_instance()?.class else {
            return None;
        };
        if !alias.origin(db).is_known(db, KnownClass::Slice) {
            return None;
        }
        let [start, stop, step] = alias.specialization(db).types(db) else {
            return None;
        };

        let to_u32 = |ty: &Type<'db>| match ty {
            Type::IntLiteral(n) => i32::try_from(*n).map(Some).ok(),
            Type::BooleanLiteral(b) => Some(Some(i32::from(*b))),
            Type::NominalInstance(instance)
                if instance.class.is_known(db, KnownClass::NoneType) =>
            {
                Some(None)
            }
            _ => None,
        };
        Some(SliceLiteral {
            start: to_u32(start)?,
            stop: to_u32(stop)?,
            step: to_u32(step)?,
        })
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
        let Place::Type(slots_ty, bound) = base.own_class_member(db, None, "__slots__").place
        else {
            return Self::NotSpecified;
        };

        if matches!(bound, Boundness::PossiblyUnbound) {
            return Self::Dynamic;
        }

        match slots_ty {
            // __slots__ = ("a", "b")
            Type::Tuple(tuple) => {
                let tuple = tuple.tuple(db);
                if tuple.is_variadic() {
                    Self::Dynamic
                } else if tuple.is_empty() {
                    Self::Empty
                } else {
                    Self::NotEmpty
                }
            }

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
    use crate::module_resolver::resolve_module;
    use crate::{PythonVersionSource, PythonVersionWithSource};
    use salsa::Setter;
    use strum::IntoEnumIterator;

    #[test]
    fn known_class_roundtrip_from_str() {
        let db = setup_db();
        for class in KnownClass::iter() {
            let class_name = class.name(&db);
            let class_module = resolve_module(&db, &class.canonical_module(&db).name()).unwrap();

            assert_eq!(
                KnownClass::try_from_file_and_name(&db, class_module.file().unwrap(), class_name),
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
            assert_ne!(
                class.to_instance(&db),
                Type::unknown(),
                "Unexpectedly fell back to `Unknown` for `{class:?}`"
            );
        }
    }

    #[test]
    fn known_class_doesnt_fallback_to_unknown_unexpectedly_on_low_python_version() {
        let mut db = setup_db();

        for class in KnownClass::iter() {
            let version_added = match class {
                KnownClass::UnionType => PythonVersion::PY310,
                KnownClass::BaseExceptionGroup | KnownClass::ExceptionGroup => PythonVersion::PY311,
                KnownClass::GenericAlias => PythonVersion::PY39,
                KnownClass::KwOnly => PythonVersion::PY310,
                KnownClass::Member | KnownClass::Nonmember => PythonVersion::PY311,
                _ => PythonVersion::PY37,
            };

            Program::get(&db)
                .set_python_version_with_source(&mut db)
                .to(PythonVersionWithSource {
                    version: version_added,
                    source: PythonVersionSource::default(),
                });

            assert_ne!(
                class.to_instance(&db),
                Type::unknown(),
                "Unexpectedly fell back to `Unknown` for `{class:?}` on Python {version_added}"
            );
        }
    }
}
