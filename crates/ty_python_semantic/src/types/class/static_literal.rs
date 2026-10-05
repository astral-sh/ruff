pub(in crate::types) mod decorators;
pub(in crate::types) mod inheritance_cycle;
pub(in crate::types) mod inner_metaclass;

use super::instance_storage::{
    InlineInstanceStorageEffects, static_instance_member_sync, static_is_typed_dict_sync,
};
use super::member_source::{
    InlineMemberSourceEffects, SynchronousStaticInstanceMemberEffects, instance_field_policy,
    static_own_instance_member_sync,
};
use super::metaclass_selection::{
    MetaclassSelectionResult, SynchronousStaticMetaclassEffects, static_inferred_metaclass_sync,
    static_metaclass_sync, static_try_metaclass_sync,
};
use super::protocol_status::{InlineProtocolStatusEffects, static_is_protocol_sync};
use crate::ProgramEnvironment;
use crate::place::PlaceFromDeclarationsResult;
use crate::types::mro::field_reads::MroFieldReads;
use decorators::{
    DecoratorFacts, InlineClassDecoratorEffects, class_decorators_sync,
    has_known_class_decorator_sync,
};
use inheritance_cycle::{OrdinaryCycleTraversal, inheritance_cycle_inner_sync};
use inner_metaclass::{OrdinaryInnerMetaclass, inherited_transform_sync, inner_metaclass_sync};
use itertools::{Either, Itertools};
use ruff_db::{
    PythonFile,
    diagnostic::Span,
    files::File,
    parsed::{ParsedModuleRef, parsed_module},
};
use ruff_python_ast as ast;
use ruff_python_ast::{PythonVersion, name::Name};
use ruff_text_size::{Ranged, TextRange};
use salsa::plumbing::function::{Configuration, IngredientImpl};
use std::convert::Infallible;
use ty_python_core::{DeclarationsIterator, ImportedFinalCandidatesIterator};

use super::base_entries::{
    ClassBaseEntryEffects, ExplicitBaseFacts, InlineClassBaseEntryEffects,
    InlineExplicitBaseEffects, expanded_class_base_entries_with, explicit_base_types_sync,
    initial_explicit_base_types_sync, recover_explicit_base_types_sync,
};
use super::base_typevars::{OrdinaryBaseTypeVarEffects, typevars_referenced_in_bases_sync};
use super::context::pep695::{InlineClassHeaderContext, class_header_context_sync};
use super::context::{
    InlineClassContextEffects, SynchronousClassContextSourceEffects, explicit_class_bases_sync,
    generic_context_with, inherited_legacy_generic_context_sync, legacy_generic_context_with,
    pep695_generic_context_sync,
};
use super::instance_flags::{InlineInstanceFlagsEffects, inherited_instance_flags_with};
use super::source::{SourceClassEffects, apply_class_specialization};
use crate::types::source_read::SourceReadControl;
use crate::{
    Db, FxIndexMap, FxIndexSet, TypeQualifiers,
    place::{
        ConsideredDefinitions, DefinedPlace, Definedness, Place, PlaceAndQualifiers,
        PublicTypePolicy, RequiresExplicitReExport, TypeOrigin, place_by_id, place_from_bindings,
        place_from_declarations,
    },
    reachability::{DeclarationsIteratorExtension, ReachabilityConstraintsExtension},
    types::{
        ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, CallableType,
        ClassBase, ClassLiteral, ClassType, DATACLASS_FLAGS, DataclassFlags, DataclassParams,
        GenericAlias, GenericContext, KnownClass, KnownInstanceType, MaterializationKind,
        MemberLookupPolicy, MetaclassTransformInfo, Parameter, Parameters, Signature,
        SpecialFormType, StaticMroError, SubclassOfType, Type, TypeContext, TypeMapping,
        TypeVarVariance, TypingModule, UnionBuilder, UnionType,
        attribute_write::DescriptorSetterDomain,
        bound_super::BoundSuperType,
        callable::CallableTypeKind,
        class::{
            ClassInstanceFlags, ClassMemberResult, ClassMetaclass, CodeGeneratorKind, DisjointBase,
            DynamicTypedDictLiteral, Field, FieldKind, MetaclassError, MetaclassErrorKind,
            MethodDecorator, MroLookup, NamedTupleField,
            member_lookup::into_function_like_callable,
            own_member::{InlineOwnMemberEffects, OwnMemberLookupRequest, own_class_member_sync},
            synthesize_namedtuple_class_member,
            synthesized_member::{
                self, SynchronousSynthesizedMemberEffects, SynthesizedMemberWork,
                own_synthesized_member_sync,
            },
            typed_dict::{TypedDictFields, synthesize_typed_dict_method, typed_dict_class_member},
        },
        context::InferContext,
        dedicated::pydantic,
        definition_expression_type, determine_upper_bound,
        diagnostic::INVALID_DATACLASS_OVERRIDE,
        enums::enum_metadata,
        function::{DataclassTransformerParams, KnownFunction},
        generics::Specialization,
        inferred_declaration,
        known_instance::DeprecatedInstance,
        member::{Member, class_member},
        mro::{
            Mro, MroIterator,
            root::{InlineMroRootEffects, apply_optional_class_specialization_sync},
        },
        signatures::CallableSignature,
        typed_dict::{TypedDictParams, TypedDictType, typed_dict_params_from_class_def},
        variance::{MemberVariance, VarianceInferable, VarianceOrigin, VarianceTerm},
    },
};
use ty_python_core::{
    ProgramFile, attribute_scopes,
    definition::{Definition, DefinitionKind, DefinitionState},
    place_table,
    scope::ScopeId,
    semantic_index, use_def_map,
};

/// Representation of a class definition statement in the AST: either a non-generic class, or a
/// generic class that has not been specialized.
///
/// This does not in itself represent a type, but can be transformed into a [`ClassType`] that
/// does. (For generic classes, this requires specializing its generic context.)
#[salsa::interned(debug, field_view=read_fields, field_requests=field_requests, heap_size=ruff_memory_usage::heap_size)]
pub struct StaticClassLiteral<'db> {
    /// Name of the class at definition
    #[returns(ref)]
    pub(crate) name: Name,

    #[returns(copy)]
    pub(crate) body_scope: ScopeId<'db>,

    #[returns(copy)]
    pub(crate) known: Option<KnownClass>,

    /// If this class is deprecated, this holds the deprecation message.
    #[returns(copy)]
    pub(crate) deprecated: Option<DeprecatedInstance<'db>>,

    #[returns(copy)]
    pub(crate) type_check_only: bool,

    #[returns(copy)]
    pub(crate) dataclass_params: Option<DataclassParams<'db>>,
    #[returns(copy)]
    pub(crate) dataclass_transformer_params: Option<DataclassTransformerParams<'db>>,

    /// Whether this class is decorated with `@functools.total_ordering`
    #[returns(copy)]
    pub(crate) total_ordering: bool,

    /// Whether this class has any decorators.
    #[returns(copy)]
    pub(crate) has_decorators: bool,

    /// Whether this class has PEP 695 type parameters.
    #[returns(copy)]
    pub(crate) has_type_params: bool,

    /// Whether this class has any explicit base classes.
    #[returns(copy)]
    pub(crate) has_explicit_bases: bool,

    /// Whether this class has an explicit `metaclass` keyword argument.
    #[returns(copy)]
    pub(crate) has_explicit_metaclass: bool,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for StaticClassLiteral<'_> {}

/// The result of [`StaticClassLiteral::inherited_frozen_dataclass_dispatch`].
///
/// See that method for details on how generated frozen-dataclass methods handle fields and
/// non-fields on subclass instances.
#[derive(Clone, Copy)]
pub(crate) enum FrozenDataclassDispatch<'db> {
    /// A reachable frozen dataclass rejects assignment to or deletion of one of its fields.
    FrozenField,
    /// Every reachable frozen method delegates, with lookup resuming after this base.
    Delegate(StaticClassLiteral<'db>),
}

impl<'db> FrozenDataclassDispatch<'db> {
    /// Returns the receiver for the next step of assignment or deletion validation.
    ///
    /// Validation stays on `object_ty` for a frozen field because the generated method rejects the
    /// mutation. For a non-field, the generated method calls `super(frozen_base, object_ty)`, so
    /// lookup must resume after the last frozen base. For example, assigning `Child().y` for
    /// `class Child(Frozen, Later)` uses `super(Frozen, child)` when `y` is not a field of `Frozen`;
    /// this preserves a later `__setattr__` or a descriptor for `y`.
    pub(crate) fn receiver(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        object_ty: Type<'db>,
    ) -> Type<'db> {
        match self {
            Self::FrozenField => object_ty,
            Self::Delegate(frozen_base) => BoundSuperType::build(
                db,
                env,
                Type::ClassLiteral(ClassLiteral::Static(frozen_base)),
                object_ty,
            )
            .unwrap_or(object_ty),
        }
    }
}

/// A method synthesized for a frozen dataclass.
#[derive(Clone, Copy)]
pub(in crate::types) enum FrozenDataclassMethod {
    SetAttr,
    DelAttr,
}

impl FrozenDataclassMethod {
    /// Returns the frozen-dataclass method for `name`, if it is `__setattr__` or `__delattr__`.
    pub(super) fn from_name(name: &str) -> Option<Self> {
        match name {
            "__setattr__" => Some(Self::SetAttr),
            "__delattr__" => Some(Self::DelAttr),
            _ => None,
        }
    }

    /// Returns the corresponding Python special-method name.
    const fn name(self) -> &'static str {
        match self {
            Self::SetAttr => "__setattr__",
            Self::DelAttr => "__delattr__",
        }
    }
}

/// Fields protected by reachable frozen-dataclass methods.
struct InheritedFrozenDataclassFields<'db> {
    names: Box<[Name]>,
    /// The final frozen dataclass whose generated method participates in dispatch.
    ///
    /// For a non-field, mutation validation resumes after this class in the MRO.
    last_frozen_base: StaticClassLiteral<'db>,
}

/// Annotated fields and class-variable declarations collected from one class body.
///
/// Class variables are not constructor parameters, but they can mask inherited dataclass fields:
///
/// ```python
/// @dataclass
/// class Child(Base):
///     value: ClassVar[int]
///     required: int
/// ```
///
/// Here, `required` is a constructor field and `value` masks an inherited `Base.value` field.
#[derive(Debug, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct OwnClassFields<'db> {
    fields: FxIndexMap<Name, Field<'db>>,
    class_variables: Box<[Name]>,
}

struct InlineSynthesizedMemberEffects<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl synthesized_member::sealed::Sealed for InlineSynthesizedMemberEffects<'_, '_> {}

impl<'db> SynchronousSynthesizedMemberEffects<'db> for InlineSynthesizedMemberEffects<'_, 'db> {
    fn total_ordering(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.total_ordering(self.db))
    }
    type Error = Infallible;

    #[inline]
    fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Infallible> {
        Ok(CodeGeneratorKind::from_class(self.db, class.into()))
    }
    #[inline]
    fn checkpoint(&self, _work: SynthesizedMemberWork) -> Result<(), Infallible> {
        Ok(())
    }

    #[inline]
    fn total_ordering_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(request.class.own_total_ordering_member(
            self.db,
            self.env,
            request.specialization,
            request.name,
        ))
    }

    #[inline]
    fn frozen_subclass_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
        method: FrozenDataclassMethod,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(request.class.own_frozen_dataclass_subclass_method(
            self.db,
            self.env,
            request.specialization,
            method,
        ))
    }

    #[inline]
    fn generated_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(request.class.own_generated_member(
            self.db,
            self.env,
            request.specialization,
            request.inherited_generic_context,
            request.name,
            field_policy,
        ))
    }
}

struct InlineClassContextSourceEffects<'db>(&'db dyn Db);

impl<'db> SynchronousClassContextSourceEffects<'db> for InlineClassContextSourceEffects<'db> {
    type Error = Infallible;

    fn has_type_params(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.has_type_params(self.0))
    }

    fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.has_explicit_bases(self.0))
    }

    fn pep695_generic_context_inner(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(pep695_generic_context_inner(self.0, class))
    }

    fn explicit_bases_inner(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        Ok(explicit_bases_inner(self.0, class))
    }

    fn inherited_legacy_generic_context_inner(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(inherited_legacy_generic_context_inner(self.0, class))
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) StaticClassGenericContextConfiguration), attempt = ReturnOnly,
    returns(copy),
    cycle_initial=|_, _, _| None,
    heap_size=ruff_memory_usage::heap_size,
)]
fn static_class_generic_context<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<GenericContext<'db>> {
    #[cfg(test)]
    if salsa::attempt_probe::is_incomplete(db) {
        return None;
    }
    #[cfg(test)]
    let _observation = crate::types::constructor::expansion_probe::observe_class_context(class);

    // This context belongs to the source declaration. A caller's specialization is applied
    // separately and does not affect how this query constructs the context.
    match generic_context_with(db, class, &InlineClassContextEffects::new(db)) {
        Ok(context) => context,
        Err(never) => match never {},
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) Pep695GenericContextInnerConfiguration), attempt = ReturnOnly,
    returns(copy),
    cycle_initial=|_, _, _| None,
    heap_size=ruff_memory_usage::heap_size,
)]
fn pep695_generic_context_inner<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<GenericContext<'db>> {
    let scope = class.body_scope(db);
    let program_file = scope.program_file(db);
    let python_file = program_file.python_file(db);
    let parsed = parsed_module(db, python_file).load(db);
    let class_def_node = scope.node(db).expect_class().node(&parsed);
    match class_header_context_sync(class_def_node, &InlineClassHeaderContext::new(db, program_file)) {
        Ok(context) => context,
        Err(never) => match never {},
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) ExplicitBasesInnerConfiguration), attempt = ReturnOnly, returns(deref), cycle_initial=explicit_bases_cycle_initial, cycle_fn=explicit_bases_cycle_fn, heap_size=ruff_memory_usage::heap_size)]
fn explicit_bases_inner<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> Box<[Type<'db>]> {
    tracing::trace!(
        "StaticClassLiteral::explicit_bases_query: {}",
        class.name(db)
    );

    // The class key owns these source expressions independently of any caller's
    // specialization. An interrupted query retains only internal recovery storage.
    explicit_base_types_with(db, class, &SourceClassEffects::new(db)).unwrap_or_default()
}

/// Returns the canonical context ingredient without preparing or certifying a memo.
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn static_class_generic_context_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<StaticClassGenericContextConfiguration> {
    static_class_generic_context::fn_ingredient_(db, db.zalsa())
}

/// Returns the existing class-header context ingredient without preparing its memo.
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn pep695_generic_context_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<Pep695GenericContextInnerConfiguration> {
    pep695_generic_context_inner::fn_ingredient_(db, db.zalsa())
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn explicit_bases_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<ExplicitBasesInnerConfiguration> {
    explicit_bases_inner::fn_ingredient_(db, db.zalsa())
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn inherited_class_context_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<InheritedLegacyGenericContextInnerConfiguration> {
    inherited_legacy_generic_context_inner::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked(configuration = (pub(in crate::types) TryMroUnspecializedConfiguration), attempt = ReturnOnly,
    returns(as_ref),
    cycle_initial=|db, _, class: StaticClassLiteral<'db>| {
        crate::types::mro::source::cycle(db, class)
    },
    heap_size=ruff_memory_usage::heap_size
)]
fn try_mro_unspecialized<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Result<Mro<'db>, Box<StaticMroError<'db>>> {
    tracing::trace!("StaticClassLiteral::try_mro: {}", class.name(db));
    crate::types::mro::source::compute(db, class).map_err(Box::new)
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn try_mro_unspecialized_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<TryMroUnspecializedConfiguration> {
    try_mro_unspecialized::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    /// Return `true` if this class represents `known_class`
    pub(crate) fn is_known(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.known(db) == Some(known_class)
    }

    pub(crate) fn is_tuple(self, db: &'db dyn Db) -> bool {
        self.is_known(db, KnownClass::Tuple)
    }

    /// Returns `true` if this class inherits from a functional namedtuple
    /// (`DynamicNamedTupleLiteral`) that has unknown fields.
    ///
    /// When the base namedtuple's fields were determined dynamically (e.g., from a variable),
    /// we can't synthesize precise method signatures and should fall back to `NamedTupleFallback`.
    fn namedtuple_base_has_unknown_fields(self, db: &'db dyn Db) -> bool {
        self.explicit_bases(db).iter().any(|base| match base {
            Type::ClassLiteral(ClassLiteral::DynamicNamedTuple(namedtuple)) => {
                !namedtuple.has_known_fields(db)
            }
            _ => false,
        })
    }

    /// Returns `true` if this class is a dataclass-like class.
    ///
    /// This covers `@dataclass`-decorated classes, as well as classes created via
    /// `dataclass_transform` (function-based, metaclass-based, and base-class-based).
    /// This specifically excludes Pydantic models, even though their metaclass also uses
    /// `dataclass_transform`.
    pub(crate) fn is_dataclass_like(self, db: &'db dyn Db) -> bool {
        CodeGeneratorKind::from_class(db, ClassLiteral::Static(self))
            .is_some_and(CodeGeneratorKind::is_dataclass_like)
    }

    /// Returns `true` if this class is decorated with `@dataclass(order=True)`.
    pub(crate) fn is_ordered_dataclass(self, db: &'db dyn Db) -> bool {
        self.find_dataclass_decorator_position(db).is_some()
            && self
                .dataclass_params(db)
                .is_some_and(|params| params.flags(db).contains(DataclassFlags::ORDER))
    }

    /// Returns a new [`StaticClassLiteral`] with the given dataclass params, preserving all other fields.
    pub(crate) fn with_dataclass_params(
        self,
        db: &'db dyn Db,
        dataclass_params: Option<DataclassParams<'db>>,
    ) -> Self {
        Self::new(
            db,
            self.name(db),
            self.body_scope(db),
            self.known(db),
            self.deprecated(db),
            self.type_check_only(db),
            dataclass_params,
            self.dataclass_transformer_params(db),
            self.total_ordering(db),
            self.has_decorators(db),
            self.has_type_params(db),
            self.has_explicit_bases(db),
            self.has_explicit_metaclass(db),
        )
    }

    /// Returns `true` if this class defines any ordering method (`__lt__`, `__le__`, `__gt__`,
    /// `__ge__`) in its own body (not inherited). Used by `@total_ordering` to determine if
    /// synthesis is valid.
    pub(crate) fn has_own_ordering_method(self, db: &'db dyn Db) -> bool {
        has_own_ordering_method(db, self)
    }

    pub(crate) fn has_own_comparison_methods(self, db: &'db dyn Db) -> bool {
        has_own_comparison_methods(db, self)
    }

    /// Returns `true` if any class in this class's MRO (excluding `object`) defines an ordering
    /// method (`__lt__`, `__le__`, `__gt__`, `__ge__`). Used by `@total_ordering` validation.
    pub(crate) fn has_ordering_method_in_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> bool {
        self.total_ordering_root_method(db, specialization)
            .is_some()
    }

    /// Returns the type of the ordering method used by `@total_ordering`, if any.
    ///
    /// Following `functools.total_ordering` precedence, we prefer `__lt__` > `__le__` > `__gt__` >
    /// `__ge__`, regardless of whether the method is defined locally or inherited.
    ///
    /// Note: We use direct scope lookups here to avoid infinite recursion
    /// through `own_class_member` -> `own_synthesized_member`.
    fn total_ordering_root_method(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Option<Type<'db>> {
        const ORDERING_METHODS: [&str; 4] = ["__lt__", "__le__", "__gt__", "__ge__"];

        for name in ORDERING_METHODS {
            for base in self.iter_mro(db, specialization) {
                let Some(base_class) = base.into_class() else {
                    continue;
                };
                match base_class.class_literal(db) {
                    ClassLiteral::Static(base_literal) => {
                        if base_literal.is_known(db, KnownClass::Object) {
                            continue;
                        }
                        let member = class_member(db, base_literal.body_scope(db), name);
                        if let Some(ty) = member.ignore_possibly_undefined() {
                            let base_specialization = base_class
                                .static_class_literal(db)
                                .and_then(|(_, spec)| spec);
                            return Some(ty.apply_optional_specialization(db, base_specialization));
                        }
                    }
                    ClassLiteral::Dynamic(dynamic) => {
                        // Dynamic classes (created with `type()`) can also define ordering methods
                        // in their namespace dict.
                        let member = dynamic.own_class_member(db, name);
                        if let Some(ty) = member.ignore_possibly_undefined() {
                            return Some(ty);
                        }
                    }
                    ClassLiteral::DynamicNamedTuple(_)
                    | ClassLiteral::DynamicTypedDict(_)
                    | ClassLiteral::DynamicEnum(_) => {}
                }
            }
        }

        None
    }

    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        static_class_generic_context(db, self)
    }

    pub(crate) fn has_pep_695_type_params(self, db: &'db dyn Db) -> bool {
        self.pep695_generic_context(db).is_some()
    }

    pub(crate) fn pep695_generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        match pep695_generic_context_sync(self, &InlineClassContextSourceEffects(db)) {
            Ok(context) => context,
            Err(never) => match never {},
        }
    }

    pub(crate) fn legacy_generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        match legacy_generic_context_with(self, &InlineClassContextEffects::new(db)) {
            Ok(context) => context,
            Err(never) => match never {},
        }
    }

    pub(crate) fn inherited_legacy_generic_context(
        self,
        db: &'db dyn Db,
    ) -> Option<GenericContext<'db>> {
        match inherited_legacy_generic_context_sync(self, &InlineClassContextSourceEffects(db)) {
            Ok(context) => context,
            Err(never) => match never {},
        }
    }

    /// Iterate through the decorators on this class, returning the span of the first one
    /// that matches the given predicate.
    fn find_decorator_span(
        self,
        db: &'db dyn Db,
        predicate: impl Fn(Type<'db>) -> bool,
    ) -> Option<Span> {
        if !self.has_decorators(db) {
            return None;
        }
        let definition = self.definition(db);
        let file = definition.file(db);
        self.node(db, &parsed_module(db, definition.python_file(db)).load(db))
            .decorator_list
            .iter()
            .find(|decorator| {
                predicate(definition_expression_type(
                    db,
                    definition,
                    &decorator.expression,
                ))
            })
            .map(|decorator| Span::from(file).with_range(decorator.range))
    }

    /// Iterate through the decorators on this class, returning the span of the first one
    /// that matches the given [`KnownFunction`].
    pub(crate) fn find_known_decorator_span(
        self,
        db: &'db dyn Db,
        needle: KnownFunction,
    ) -> Option<Span> {
        self.find_decorator_span(db, |ty| {
            ty.as_function_literal()
                .is_some_and(|f| f.is_known(db, needle))
        })
    }

    /// Returns all of the typevars that are referenced in this class's base class list.
    /// (This is used to ensure that classes do not reference typevars from enclosing
    /// generic contexts.)
    pub(crate) fn typevars_referenced_in_bases(
        self,
        db: &'db dyn Db,
    ) -> FxIndexSet<BoundTypeVarInstance<'db>> {
        match typevars_referenced_in_bases_sync(self, &OrdinaryBaseTypeVarEffects::new(db)) {
            Ok(variables) => variables,
            Err(never) => match never {},
        }
    }

    /// Returns the generic context that should be inherited by any constructor methods of this class.
    pub(in crate::types) fn inherited_generic_context(
        self,
        db: &'db dyn Db,
    ) -> Option<GenericContext<'db>> {
        self.generic_context(db)
    }

    pub(crate) fn file(self, db: &dyn Db) -> File {
        self.body_scope(db).file(db)
    }

    pub(crate) fn python_file(self, db: &'db dyn Db) -> PythonFile<'db> {
        self.body_scope(db).python_file(db)
    }

    pub(crate) fn program_file(self, db: &'db dyn Db) -> ProgramFile<'db> {
        self.body_scope(db).program_file(db)
    }

    /// Return the original [`ast::StmtClassDef`] node associated with this class
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn node<'ast>(self, db: &'db dyn Db, module: &'ast ParsedModuleRef) -> &'ast ast::StmtClassDef {
        self.body_scope(db).node(db).expect_class().node(module)
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let body_scope = self.body_scope(db);
        let index = semantic_index(db, body_scope.program_file(db));
        index.expect_single_definition(body_scope.node(db).expect_class())
    }

    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> ClassType<'db> {
        apply_class_specialization(db, self, f)
    }

    pub(crate) fn apply_optional_specialization(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> ClassType<'db> {
        #[cfg(test)]
        if crate::types::constructor::expansion_probe::mro_effects_enabled() {
            return apply_optional_class_specialization_sync(
                db,
                self,
                specialization,
                &crate::types::mro::attempt::AttemptMroEffects::new(db),
            )
            .unwrap_or_else(|_| ClassType::NonGeneric(self.into()));
        }
        match apply_optional_class_specialization_sync(
            db,
            self,
            specialization,
            &InlineMroRootEffects::new(db),
        ) {
            Ok(class) => class,
            Err(never) => match never {},
        }
    }

    pub(crate) fn top_materialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            let env = ProgramEnvironment::from_program(generic_context.program(db));
            generic_context
                .unknown_specialization(db, self.known(db))
                .materialize_impl(
                    db,
                    MaterializationKind::Top,
                    &ApplyTypeMappingVisitor::new(&env),
                )
        })
    }

    /// Returns the default specialization of this class. For non-generic classes, the class is
    /// returned unchanged. For a non-specialized generic class, we return a generic alias that
    /// applies the default specialization to the class's typevars.
    pub(crate) fn default_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_optional_specialization(db, None)
    }

    /// Returns the unknown specialization of this class. For non-generic classes, the class is
    /// returned unchanged. For a non-specialized generic class, we return a generic alias that
    /// maps each of the class's typevars to `Unknown`.
    pub(crate) fn unknown_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            generic_context.unknown_specialization(db, self.known(db))
        })
    }

    /// Returns a specialization of this class where each typevar is mapped to itself.
    pub(crate) fn identity_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        match super::identity::class_identity_specialization_sync(
            self,
            &super::identity::OrdinaryClassIdentityEffects { db },
        ) {
            Ok(class) => class,
            Err(never) => match never {},
        }
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
    pub(crate) fn explicit_bases(self, db: &'db dyn Db) -> &'db [Type<'db>] {
        match explicit_class_bases_sync(self, &InlineClassContextSourceEffects(db)) {
            Ok(bases) => bases,
            Err(never) => match never {},
        }
    }

    /// Return `Some()` if this class is known to be a [`DisjointBase`], or `None` if it is not.
    pub(super) fn as_disjoint_base(self, db: &'db dyn Db) -> Option<DisjointBase<'db>> {
        if self
            .known_function_decorators(db)
            .contains(&KnownFunction::DisjointBase)
            && !self.is_typed_dict(db)
            && !self.is_protocol(db)
        {
            Some(DisjointBase::due_to_decorator(self))
        } else if self.has_nonempty_slots(db) {
            Some(DisjointBase::due_to_dunder_slots(ClassLiteral::Static(
                self,
            )))
        } else {
            None
        }
    }

    /// Determine if this class is a protocol.
    ///
    /// This method relies on the accuracy of the [`KnownClass::is_protocol`] method,
    /// which hardcodes knowledge about certain special-cased classes. See the docs on
    /// that method for why we do this rather than relying on generalised logic for all
    /// classes, including the special-cased ones that are included in the [`KnownClass`]
    /// enum.
    pub(crate) fn is_protocol(self, db: &'db dyn Db) -> bool {
        match static_is_protocol_sync(self, &InlineProtocolStatusEffects { db }) {
            Ok(status) => status,
            Err(never) => match never {},
        }
    }

    pub(in crate::types) fn protocol_explicit_bases(bases: &[Type<'_>]) -> bool {
        // Iterate through the last three bases of the class
        // searching for `Protocol` or `Protocol[]` in the bases list.
        //
        // If `Protocol` is present in the bases list of a valid protocol class, it must either:
        //
        // - be the last base
        // - OR be the last-but-one base (with the final base being `Generic[]` or `object`)
        // - OR be the last-but-two base (with the penultimate base being `Generic[]`
        //                                and the final base being `object`)
        bases.iter().rev().take(3).any(|base| {
            matches!(
                base,
                Type::SpecialForm(SpecialFormType::Protocol)
                    | Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(_))
            )
        })
    }

    /// Return protocol classification when the stored class header settles it without base inference.
    pub(in crate::types) fn is_protocol_without_inference(self, db: &'db dyn Db) -> Option<bool> {
        self.known(db)
            .map(KnownClass::is_protocol)
            .or_else(|| (!self.has_explicit_bases(db)).then_some(false))
    }

    /// Return the types of the decorators on this class
    fn decorators(self, db: &'db dyn Db) -> &'db [Type<'db>] {
        if !self.has_decorators(db) {
            return &[];
        }
        self.decorators_inner(db)
    }

    fn decorators_inner(self, db: &'db dyn Db) -> &'db [Type<'db>] {
        decorators_inner_(db, self)
    }

    pub(crate) fn known_function_decorators(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = KnownFunction> + 'db {
        self.decorators(db)
            .iter()
            .filter_map(|deco| deco.as_function_literal())
            .filter_map(|decorator| decorator.known(db))
    }

    /// Iterate through the decorators on this class, returning the index of the first one
    /// that is either `@dataclass` or `@dataclass(...)`.
    pub(crate) fn find_dataclass_decorator_position(self, db: &'db dyn Db) -> Option<usize> {
        let program_file = self.program_file(db);
        let python_file = program_file.python_file(db);
        let module = parsed_module(db, python_file).load(db);
        let class_stmt = self.node(db, &module);
        let class_definition =
            semantic_index(db, program_file).expect_single_definition(class_stmt);

        class_stmt.decorator_list.iter().position(|decorator| {
            let decorator_callable = decorator
                .expression
                .as_call_expr()
                .map_or(&decorator.expression, |call| &call.func);

            definition_expression_type(db, class_definition, decorator_callable)
                .as_function_literal()
                .is_some_and(|function| function.is_known(db, KnownFunction::Dataclass))
        })
    }

    /// Is this class final?
    pub(crate) fn is_final(self, db: &'db dyn Db) -> bool {
        match static_finality_sync(self, &InlineStaticFinality(db)) {
            Ok(result) => result,
            Err(error) => match error {},
        }
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
    pub(in crate::types) fn try_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Result<&'db Mro<'db>, &'db StaticMroError<'db>> {
        match specialization {
            None => self.try_mro_unspecialized(db),
            Some(specialization) => GenericAlias::new(db, self, specialization).try_mro(db),
        }
        .map_err(Box::as_ref)
    }

    fn try_mro_unspecialized(
        self,
        db: &'db dyn Db,
    ) -> Result<&'db Mro<'db>, &'db Box<StaticMroError<'db>>> {
        try_mro_unspecialized(db, self)
    }

    /// Iterate over the [method resolution order] ("MRO") of the class.
    ///
    /// If the MRO could not be accurately resolved, this method falls back to iterating
    /// over an MRO that has the class directly inheriting from `Unknown`. Use
    /// [`StaticClassLiteral::try_mro`] if you need to distinguish between the success and failure
    /// cases rather than simply iterating over the inferred resolution order for the class.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(crate) fn iter_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> MroIterator<'db> {
        MroIterator::new(db, ClassLiteral::Static(self), specialization)
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

    /// Return the properties shared by all instances of this class.
    pub(super) fn instance_flags(self, db: &'db dyn Db) -> ClassInstanceFlags {
        #[cfg(test)]
        if crate::types::constructor::expansion_probe::mro_effects_enabled() {
            if salsa::attempt_probe::is_incomplete(db) {
                return ClassInstanceFlags::empty();
            }
            return self
                .instance_flags_with(
                    db,
                    &super::instance_flags::AttemptInstanceFlagsEffects::new(db),
                )
                .unwrap_or_default();
        }
        match self.instance_flags_with(db, &InlineInstanceFlagsEffects::new(db)) {
            Ok(flags) => flags,
            Err(never) => match never {},
        }
    }

    pub(super) fn inherited_instance_flags(self, db: &'db dyn Db) -> ClassInstanceFlags {
        instance_flags_inner(db, self)
    }

    /// Return the module defining the `TypedDict` base of this class.
    pub(crate) fn typed_dict_module(self, db: &'db dyn Db) -> Option<TypingModule> {
        typed_dict_module(db, self)
    }

    /// Return `true` if this class constitutes a typed dict specification (inherits from
    /// `typing.TypedDict` or `typing_extensions.TypedDict`, either directly or indirectly).
    pub fn is_typed_dict(self, db: &'db dyn Db) -> bool {
        match static_is_typed_dict_sync(self, &InlineInstanceStorageEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Return `TypedDict` classification when the stored class header settles it without base inference.
    pub(in crate::types) fn is_typed_dict_without_inference(self, db: &'db dyn Db) -> Option<bool> {
        MroFieldReads::new(db).typed_dict_without_inference(self)
    }

    /// Return `true` if this class is, or inherits from, a `NamedTuple` (inherits from
    /// `typing.NamedTuple`, either directly or indirectly, including functional forms like
    /// `NamedTuple("X", ...)`).
    pub(crate) fn has_named_tuple_class_in_mro(self, db: &'db dyn Db) -> bool {
        self.iter_mro(db, None)
            .filter_map(ClassBase::into_class)
            .any(|base| match base.class_literal(db) {
                ClassLiteral::DynamicNamedTuple(_) => true,
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_) => false,
                ClassLiteral::Static(class) => class
                    .explicit_bases(db)
                    .contains(&Type::SpecialForm(SpecialFormType::NamedTuple)),
            })
    }

    /// Compute `TypedDict` parameters dynamically based on MRO detection and AST parsing.
    fn typed_dict_params(self, db: &'db dyn Db) -> Option<TypedDictParams> {
        if !self.is_typed_dict(db) {
            return None;
        }

        let module = parsed_module(db, self.python_file(db)).load(db);
        let class_stmt = self.node(db, &module);
        Some(typed_dict_params_from_class_def(class_stmt))
    }

    /// Returns dataclass params for this class, sourced from both dataclass params and dataclass
    /// transform params
    fn merged_dataclass_params(
        self,
        db: &'db dyn Db,
        field_policy: CodeGeneratorKind<'db>,
    ) -> (Option<DataclassParams<'db>>, Option<DataclassParams<'db>>) {
        let dataclass_params = self.dataclass_params(db);

        let mut transformer_params =
            field_policy
                .dataclass_transformer_params()
                .map(|transformer_params| {
                    DataclassParams::from_transformer_params(db, transformer_params)
                });

        // Dataclass transformer flags can be overwritten using class arguments.
        if let Some(transformer_params) = transformer_params.as_mut()
            && let Some(class_def) = self.definition(db).kind(db).as_class()
        {
            let module = parsed_module(db, self.python_file(db)).load(db);

            if let Some(arguments) = &class_def.node(&module).arguments {
                let mut flags = transformer_params.flags(db);

                for ast::Keyword { arg, value, .. } in &arguments.keywords {
                    if let Some(arg_name) = arg
                        && let ast::Expr::BooleanLiteral(is_set) = value
                    {
                        for (flag_name, flag) in DATACLASS_FLAGS {
                            if arg_name == *flag_name {
                                flags.set(*flag, is_set.value);
                            }
                        }
                    }
                }

                *transformer_params =
                    DataclassParams::new(db, flags, transformer_params.field_specifiers(db));
            }
        }

        (dataclass_params, transformer_params)
    }

    /// Returns the effective frozen status of this class if it's a dataclass-like class.
    ///
    /// Returns `Some(true)` for a frozen dataclass-like class, `Some(false)` for a non-frozen one,
    /// and `None` if the class is not a dataclass-like class, or if the dataclass is neither frozen
    /// nor non-frozen.
    pub(crate) fn is_frozen_dataclass(self, db: &'db dyn Db) -> Option<bool> {
        // Check if this is a base-class-based transformer that has dataclass_transformer_params directly
        // attached to it (because it is itself decorated with `@dataclass_transform`), or if this class
        // has an explicit metaclass that is decorated with `@dataclass_transform`.
        //
        // In both cases, this signifies that this class is neither frozen nor non-frozen.
        //
        // See <https://typing.python.org/en/latest/spec/dataclasses.html#dataclass-semantics> for details.
        if self.dataclass_transformer_params(db).is_some()
            || self
                .try_metaclass(db)
                .is_ok_and(|(_, info)| info.is_some_and(|i| i.from_explicit_metaclass))
        {
            return None;
        }

        if let field_policy @ CodeGeneratorKind::DataclassLike(_) =
            CodeGeneratorKind::from_class(db, self.into())?
        {
            // Otherwise, if this class is a dataclass-like class, determine its frozen status based on
            // dataclass params and dataclass transformer params.
            Some(self.has_dataclass_param(db, field_policy, DataclassFlags::FROZEN))
        } else {
            None
        }
    }

    /// Checks if the given dataclass parameter flag is set for this class.
    /// This checks both the `dataclass_params` and `transformer_params`.
    pub(crate) fn has_dataclass_param(
        self,
        db: &'db dyn Db,
        field_policy: CodeGeneratorKind<'db>,
        param: DataclassFlags,
    ) -> bool {
        let (dataclass_params, transformer_params) = self.merged_dataclass_params(db, field_policy);
        dataclass_params.is_some_and(|params| params.flags(db).contains(param))
            || transformer_params.is_some_and(|params| params.flags(db).contains(param))
    }

    /// Returns the nearest `@dataclass_transform` parameters for this class or its MRO.
    ///
    /// This is used for metaclass-based transforms because `__dataclass_transform__` is inherited,
    /// so a metaclass subclass should preserve the transform metadata of its decorated base class
    /// unless it provides its own.
    fn inherited_dataclass_transformer_params(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Option<DataclassTransformerParams<'db>> {
        match inherited_transform_sync(self, specialization, &OrdinaryInnerMetaclass(db)) {
            Ok(params) => params,
            Err(never) => match never {},
        }
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
    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        match static_metaclass_sync(self, &InlineStaticMetaclassEffects(db)) {
            Ok(metaclass) => metaclass,
            Err(never) => match never {},
        }
    }

    pub(in crate::types) fn inferred_metaclass(self, db: &'db dyn Db) -> ClassMetaclass<'db> {
        match static_inferred_metaclass_sync(self, &InlineStaticMetaclassEffects(db)) {
            Ok(metaclass) => metaclass,
            Err(never) => match never {},
        }
    }

    /// Return the selected metaclass or protocol fallback, or an error if it cannot be inferred.
    pub(in crate::types) fn try_metaclass(
        self,
        db: &'db dyn Db,
    ) -> Result<(ClassMetaclass<'db>, Option<MetaclassTransformInfo<'db>>), MetaclassError<'db>>
    {
        match static_try_metaclass_sync(self, &InlineStaticMetaclassEffects(db)) {
            Ok(metaclass) => metaclass,
            Err(never) => match never {},
        }
    }

    pub(in crate::types) fn has_default_metaclass(self, db: &'db dyn Db) -> bool {
        !self.has_explicit_bases(db) && !self.has_explicit_metaclass(db)
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member on the class itself or any of its proper superclasses.
    ///
    /// TODO: Should this be made private...?
    pub(super) fn class_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        self.class_member_inner(db, env, None, name, policy)
    }

    pub(super) fn class_member_inner(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        // An unspecialized MRO retains mappings such as `Parent[T@Child]`, so ordinary members
        // accessed through `Child` must use its default arguments. Constructor methods are different:
        // we add their class's type variables to the callable's generic context, so those variables
        // are genuinely inferable and must remain generic instead of using the default arguments.
        if specialization.is_none()
            && let Some(generic_context) = self.generic_context(db)
        {
            match name {
                "__new__" | "__init__" => {
                    // Specifically apply the identity specialization; otherwise `iter_mro` will
                    // apply the default specialization for us.
                    let specialization = generic_context.identity_specialization(db);
                    self.class_member_from_mro(
                        db,
                        env,
                        name,
                        policy,
                        self.iter_mro(db, Some(specialization)),
                    )
                }
                _ => {
                    let member =
                        self.class_member_from_mro(db, env, name, policy, self.iter_mro(db, None));
                    let specialization = generic_context.default_specialization(db, self.known(db));
                    // An inherited method's `Self` bound can still contain this class's type
                    // variables, so the default arguments must also specialize that bound.
                    member.map_type(|ty| {
                        ty.apply_optional_owner_specialization_to_member(db, Some(specialization))
                    })
                }
            }
        } else {
            self.class_member_from_mro(db, env, name, policy, self.iter_mro(db, specialization))
        }
    }

    pub(crate) fn class_member_from_mro(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        mro_iter: impl Iterator<Item = ClassBase<'db>>,
    ) -> PlaceAndQualifiers<'db> {
        let result = MroLookup::new(db, env, mro_iter).class_member(
            name,
            policy,
            self.inherited_generic_context(db),
            self.is_known(db, KnownClass::Object),
        );

        let mut member = match result {
            ClassMemberResult::Done(result) => result.finalize(db, env),
            ClassMemberResult::TypedDict(module) => typed_dict_class_member(
                db,
                env,
                self.identity_specialization(db),
                module,
                policy,
                name,
            ),
        };

        // We generally treat dunder attributes with `Callable` types as function-like callables.
        // See `callables_as_descriptors.md` for more details.
        if name.starts_with("__") && name.ends_with("__") {
            member = member.map_type(|ty| into_function_like_callable(db, env, ty));
        }

        member
    }

    /// Returns the inferred type of the class member named `name`. Only bound members
    /// or those marked as `ClassVars` are considered.
    ///
    /// Returns [`Place::Undefined`] if `name` cannot be found in this class's scope
    /// directly. Use [`StaticClassLiteral::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(super) fn own_class_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inherited_generic_context: Option<GenericContext<'db>>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Member<'db> {
        match own_class_member_sync(
            OwnMemberLookupRequest {
                class: self,
                name,
                inherited_generic_context,
                specialization,
            },
            &InlineOwnMemberEffects::new(db, env),
        ) {
            Ok(member) => member,
            Err(never) => match never {},
        }
    }

    /// Returns the type of a synthesized dataclass member like `__init__` or `__lt__`, or
    /// a synthesized `__new__` method for a `NamedTuple`.
    pub(crate) fn own_synthesized_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        inherited_generic_context: Option<GenericContext<'db>>,
        name: &str,
    ) -> Option<Type<'db>> {
        match own_synthesized_member_sync(
            OwnMemberLookupRequest {
                class: self,
                name,
                inherited_generic_context,
                specialization,
            },
            &InlineSynthesizedMemberEffects { db, env },
        ) {
            Ok(member) => member,
            Err(never) => match never {},
        }
    }

    fn own_total_ordering_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Option<Type<'db>> {
        // Only synthesize methods that are not already defined in the MRO.
        // Note: We use direct scope lookups here to avoid infinite recursion
        // through `own_class_member` -> `own_synthesized_member`.
        if !self
            .iter_mro(db, specialization)
            .filter_map(ClassBase::into_class)
            .filter_map(|class| class.static_class_literal(db))
            .filter(|(class, _)| !class.is_known(db, KnownClass::Object))
            .any(|(class, _)| {
                class_member(db, class.body_scope(db), name)
                    .ignore_possibly_undefined()
                    .is_some()
            })
            && self.has_ordering_method_in_mro(db, specialization)
            && let Some(root_method_ty) = self.total_ordering_root_method(db, specialization)
            && let Some(callables) = root_method_ty.try_upcast_to_callable(db, env)
        {
            let bool_ty = KnownClass::Bool.to_instance(db, env);
            let synthesized_callables = callables.map(|callable| {
                let signatures = CallableSignature::from_overloads(
                    callable.signatures(db).iter().map(|signature| {
                        // The generated methods return a union of the root method's return type
                        // and `bool`. This is because `@total_ordering` synthesizes methods like:
                        //     def __gt__(self, other): return not (self == other or self < other)
                        // If `__lt__` returns `int`, then `__gt__` could return `int | bool`.
                        let return_ty =
                            UnionType::from_two_elements(db, env, signature.return_ty, bool_ty);
                        Signature::new_generic(
                            signature.generic_context,
                            signature.parameters().clone(),
                            return_ty,
                        )
                    }),
                );
                CallableType::new(db, signatures, CallableTypeKind::FunctionLike)
            });

            return Some(synthesized_callables.to_type(db, env));
        }

        None
    }

    fn own_generated_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        inherited_generic_context: Option<GenericContext<'db>>,
        name: &str,
        field_policy: CodeGeneratorKind<'db>,
    ) -> Option<Type<'db>> {
        let pydantic_constructor_fields_are_keyword_only =
            field_policy.is_pydantic() && pydantic::constructor_fields_are_keyword_only(db, self);
        let pydantic_constructor_fields_are_optional = name == "__init__"
            && field_policy.is_pydantic()
            && pydantic::constructor_fields_are_optional(db, self);

        let instance_ty = Type::instance(
            db,
            env,
            self.apply_optional_specialization(db, specialization),
        );

        let signature_from_fields = |mut parameters: Vec<_>, return_ty: Type<'db>| {
            if name == "__init__" && field_policy.is_pydantic() {
                pydantic::extend_settings_constructor_parameters(db, self, &mut parameters);
            }

            for (field_name, field) in self.fields(db, specialization, field_policy) {
                let (init, mut default_ty, kw_only, alias, converter, strict) = match &field.kind {
                    FieldKind::NamedTuple { default_ty } => (
                        true,
                        *default_ty,
                        None,
                        None,
                        None,
                        pydantic::ConfigBoolean::Unspecified,
                    ),
                    FieldKind::Dataclass {
                        init,
                        default_ty,
                        kw_only,
                        alias,
                        converter,
                        ..
                    } => (
                        *init,
                        *default_ty,
                        *kw_only,
                        alias.as_ref(),
                        *converter,
                        pydantic::ConfigBoolean::Unspecified,
                    ),
                    FieldKind::Pydantic {
                        init,
                        default_ty,
                        alias,
                        strict,
                    } => (*init, *default_ty, None, alias.as_ref(), None, *strict),
                    FieldKind::TypedDict { .. } => continue,
                };
                let mut field_ty = field.declared_ty;

                if !init && (name == "__init__" || field_policy.is_pydantic()) {
                    // Fields with `init=False` are excluded from constructors. Pydantic's private
                    // and internal fields are also excluded from replacement.
                    continue;
                }

                if field.is_kw_only_sentinel(db) {
                    // Attributes annotated with `dataclass.KW_ONLY` are not present in the synthesized
                    // `__init__` method; they are used to indicate that the following parameters are
                    // keyword-only.
                    continue;
                }

                let dunder_set = field_ty.class_member(db, env, "__set__");
                if let Place::Defined(DefinedPlace {
                    ty: dunder_set,
                    definedness: Definedness::AlwaysDefined,
                    ..
                }) = dunder_set.place
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
                        // the `__set__` method can be called.
                        //
                        // We union parameter types across overloads of a single callable, intersect
                        // callable bindings inside an intersection element, and union outer elements.
                        field_ty = dunder_set.bindings(db, env).map_types(db, env, |binding| {
                            let mut value_types = UnionBuilder::new(db, env);
                            let mut has_value_type = false;
                            for overload in binding {
                                if let Some(value_param) =
                                    overload.signature.parameters().get_positional(2)
                                {
                                    value_types = value_types.add(value_param.annotated_type());
                                    has_value_type = true;
                                } else if overload.signature.parameters().is_gradual() {
                                    value_types = value_types.add(Type::unknown());
                                    has_value_type = true;
                                }
                            }
                            has_value_type.then(|| value_types.build())
                        });

                        // The default value of the attribute is *not* determined by the right hand side
                        // of the class-body assignment. Instead, the runtime invokes `__get__` on the
                        // descriptor, as if it had been called on the class itself, i.e. it passes `None`
                        // for the `instance` argument.

                        if let Some(ref mut default_ty) = default_ty {
                            *default_ty = default_ty
                                .try_call_dunder_get(db, env, None, Type::from(self))
                                .unwrap_or_else(|error| Some(error.fallback()))
                                .map(|result| result.return_type)
                                .unwrap_or_else(Type::unknown);
                        }
                    }
                }

                if let Some((converter_input_ty, _)) = converter {
                    field_ty = converter_input_ty;
                }

                if name == "__init__"
                    && let Some(metadata) = field_policy.pydantic_metadata()
                {
                    field_ty = pydantic::constructor_parameter_type(
                        db, self, field_name, field_ty, strict, metadata,
                    );
                }

                if pydantic_constructor_fields_are_optional && default_ty.is_none() {
                    default_ty = Some(Type::unknown());
                }

                let is_kw_only = matches!(name, "__replace__" | "_replace")
                    || pydantic_constructor_fields_are_keyword_only
                    || kw_only.unwrap_or(false);

                let mut add_parameter_with_name = |parameter_name, default_ty| {
                    let mut parameter = if is_kw_only {
                        Parameter::keyword_only(parameter_name)
                    } else {
                        Parameter::positional_or_keyword(parameter_name)
                    }
                    .with_annotated_type(field_ty)
                    .with_definition(field.first_declaration);

                    parameter = if matches!(name, "__replace__" | "_replace") {
                        // When replacing, we know there is a default value for the field
                        // (the value that is currently assigned to the field)
                        // assume this to be the declared type of the field
                        parameter.with_default_type(field_ty)
                    } else {
                        parameter.with_optional_default_type(default_ty)
                    };

                    parameters.push(parameter);
                };

                if name == "__init__"
                    && let Some(metadata) = field_policy.pydantic_metadata()
                    && let Some(alias) = alias
                {
                    match (
                        metadata.validates_by_alias(db),
                        metadata.validates_by_name(db),
                    ) {
                        (true, true) => {
                            let alias = Name::new(&**alias);
                            if alias == *field_name {
                                add_parameter_with_name(field_name.clone(), default_ty);
                            } else {
                                // A normal signature cannot express that at least one of two
                                // differently named parameters is required. We could solve
                                // this with overloads, but the number of overloads would grow
                                // exponentially in the number of parameters. So for now, we
                                // treat both the alias and the field name as optional
                                // parameters, which leads to false negatives if none of them
                                // is provided.
                                let default_ty = Some(default_ty.unwrap_or_else(Type::unknown));
                                add_parameter_with_name(alias, default_ty);
                                add_parameter_with_name(field_name.clone(), default_ty);
                            }
                        }
                        (true, false) => {
                            add_parameter_with_name(Name::new(&**alias), default_ty);
                        }
                        (false, true) => {
                            add_parameter_with_name(field_name.clone(), default_ty);
                        }
                        (false, false) => {}
                    }
                } else if name == "__replace__" && field_policy.is_pydantic() {
                    // Pydantic updates model fields by name rather than by initialization alias.
                    add_parameter_with_name(field_name.clone(), default_ty);
                } else {
                    // Use the alias name if provided, otherwise use the field name.
                    let parameter_name = alias.map_or_else(|| field_name.clone(), Name::new);
                    add_parameter_with_name(parameter_name, default_ty);
                }
            }

            // In the event that we have a mix of keyword-only and positional parameters, we need to sort them
            // so that the keyword-only parameters appear after positional parameters.
            parameters.sort_by_key(Parameter::is_keyword_only);

            if name == "__init__"
                && field_policy
                    .pydantic_metadata()
                    .is_some_and(|metadata| pydantic::model_init_accepts_extra(db, self, metadata))
            {
                let extra = pydantic::extra_parameter(&parameters);
                parameters.push(extra);
            }

            let signature = match name {
                "__new__" | "__init__" => Signature::new_generic(
                    inherited_generic_context.or_else(|| self.inherited_generic_context(db)),
                    Parameters::standard(parameters),
                    return_ty,
                ),
                _ => Signature::new(Parameters::standard(parameters), return_ty),
            };
            Some(Type::function_like_callable(db, signature))
        };

        match (field_policy, name) {
            (field_policy, "__init__")
                if field_policy.synthesizes_constructor_signature_from_fields(db, self) =>
            {
                if field_policy.is_dataclass_like()
                    && !self.has_dataclass_param(db, field_policy, DataclassFlags::INIT)
                {
                    return None;
                }

                let self_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                    // TODO: could be `Self`.
                    .with_annotated_type(instance_ty);
                signature_from_fields(vec![self_parameter], Type::none(db, env))
            }
            (
                CodeGeneratorKind::NamedTuple,
                "__new__" | "__init__" | "__match_args__" | "_replace" | "__replace__" | "_fields",
            ) if self.namedtuple_base_has_unknown_fields(db) => {
                // When the namedtuple base has unknown fields, fall back to NamedTupleFallback
                // which has generic signatures that accept any arguments.
                KnownClass::NamedTupleFallback
                    .to_class_literal(db, env)
                    .as_class_literal()?
                    .as_static()?
                    .own_class_member(db, env, inherited_generic_context, None, name)
                    .ignore_possibly_undefined()
                    .map(|ty| {
                        ty.apply_type_mapping(
                            db,
                            env,
                            &TypeMapping::ReplaceSelf {
                                new_upper_bound: instance_ty,
                            },
                            TypeContext::default(),
                        )
                    })
            }
            (
                CodeGeneratorKind::NamedTuple,
                "__match_args__" | "__new__" | "_replace" | "__replace__" | "_fields" | "__slots__",
            ) => {
                let fields = self.fields(db, specialization, field_policy);
                let fields_iter = fields.iter().map(|(name, field)| {
                    let default_ty = match &field.kind {
                        FieldKind::NamedTuple { default_ty } => *default_ty,
                        _ => None,
                    };
                    NamedTupleField {
                        name: name.clone(),
                        ty: field.declared_ty,
                        default: default_ty,
                        definition: field.first_declaration,
                    }
                });
                synthesize_namedtuple_class_member(
                    db,
                    env,
                    name,
                    instance_ty,
                    fields_iter,
                    specialization.map(|s| s.generic_context(db)),
                )
            }
            (
                field_policy @ CodeGeneratorKind::DataclassLike(_),
                "__lt__" | "__le__" | "__gt__" | "__ge__",
            ) => {
                if !self.has_dataclass_param(db, field_policy, DataclassFlags::ORDER) {
                    return None;
                }

                let signature = Signature::new(
                    Parameters::standard([
                        Parameter::positional_or_keyword(Name::new_static("self"))
                            // TODO: could be `Self`.
                            .with_annotated_type(instance_ty),
                        Parameter::positional_or_keyword(Name::new_static("other"))
                            // TODO: could be `Self`.
                            .with_annotated_type(instance_ty),
                    ]),
                    KnownClass::Bool.to_instance(db, env),
                );

                Some(Type::function_like_callable(db, signature))
            }
            (field_policy @ CodeGeneratorKind::DataclassLike(_), "__hash__") => {
                let unsafe_hash =
                    self.has_dataclass_param(db, field_policy, DataclassFlags::UNSAFE_HASH);
                let frozen = self.has_dataclass_param(db, field_policy, DataclassFlags::FROZEN);
                let eq = self.has_dataclass_param(db, field_policy, DataclassFlags::EQ);

                if unsafe_hash || (frozen && eq) {
                    let signature = Signature::new(
                        Parameters::standard([Parameter::positional_or_keyword(Name::new_static(
                            "self",
                        ))
                        .with_annotated_type(instance_ty)]),
                        KnownClass::Int.to_instance(db, env),
                    );

                    Some(Type::function_like_callable(db, signature))
                } else if eq && !frozen {
                    Some(Type::none(db, env))
                } else {
                    // No `__hash__` is generated, fall back to `object.__hash__`
                    None
                }
            }
            (field_policy @ CodeGeneratorKind::DataclassLike(_), "__match_args__")
                if env.python_version(db) >= PythonVersion::PY310 =>
            {
                if !self.has_dataclass_param(db, field_policy, DataclassFlags::MATCH_ARGS) {
                    return None;
                }

                let kw_only_default =
                    self.has_dataclass_param(db, field_policy, DataclassFlags::KW_ONLY);

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
                Some(Type::heterogeneous_tuple(db, env, match_args))
            }
            (CodeGeneratorKind::NamedTuple, name) if name != "__init__" => {
                KnownClass::NamedTupleFallback
                    .to_class_literal(db, env)
                    .as_class_literal()?
                    .as_static()?
                    .own_class_member(db, env, self.inherited_generic_context(db), None, name)
                    .ignore_possibly_undefined()
                    .map(|ty| {
                        ty.apply_type_mapping(
                            db,
                            env,
                            &TypeMapping::ReplaceSelf {
                                new_upper_bound: determine_upper_bound(
                                    db,
                                    env,
                                    self.apply_optional_specialization(db, specialization),
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
            (
                CodeGeneratorKind::DataclassLike(_) | CodeGeneratorKind::Pydantic(_),
                "__replace__",
            ) if env.python_version(db) >= PythonVersion::PY313 => {
                let self_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                    .with_annotated_type(instance_ty);

                signature_from_fields(vec![self_parameter], instance_ty)
            }
            (CodeGeneratorKind::DataclassLike(_), "__setattr__") => {
                if self.is_frozen_dataclass(db) == Some(true) {
                    let signature = Signature::new(
                        Parameters::standard([
                            Parameter::positional_or_keyword(Name::new_static("self"))
                                .with_annotated_type(instance_ty),
                            Parameter::positional_or_keyword(Name::new_static("name")),
                            Parameter::positional_or_keyword(Name::new_static("value")),
                        ]),
                        Type::Never,
                    );

                    return Some(Type::function_like_callable(db, signature));
                }
                None
            }
            (CodeGeneratorKind::DataclassLike(_), "__delattr__")
                if self.is_frozen_dataclass(db) == Some(true) =>
            {
                let signature = Signature::new(
                    Parameters::standard([
                        Parameter::positional_or_keyword(Name::new_static("self"))
                            .with_annotated_type(instance_ty),
                        Parameter::positional_or_keyword(Name::new_static("name")),
                    ]),
                    Type::Never,
                );

                Some(Type::function_like_callable(db, signature))
            }
            (field_policy @ CodeGeneratorKind::DataclassLike(_), "__slots__")
                if env.python_version(db) >= PythonVersion::PY310 =>
            {
                self.has_dataclass_param(db, field_policy, DataclassFlags::SLOTS)
                    .then(|| {
                        if let Some(slots) = self.slot_names(db) {
                            return Type::heterogeneous_tuple(
                                db,
                                env,
                                slots.iter().map(|name| Type::string_literal(db, name)),
                            );
                        }

                        let fields = self.fields(db, specialization, field_policy);
                        let slots = fields.keys().map(|name| Type::string_literal(db, name));
                        Type::heterogeneous_tuple(db, env, slots)
                    })
            }
            (CodeGeneratorKind::TypedDict, name) => synthesize_typed_dict_method(
                db,
                env,
                instance_ty
                    .as_typed_dict()
                    .expect("TypedDict code generation should use a TypedDict instance"),
                name,
                || TypedDictFields::Static(self.fields(db, specialization, field_policy)),
            ),
            _ => None,
        }
    }

    /// Synthesize a `__setattr__` or `__delattr__` view for an ordinary subclass of a frozen
    /// dataclass.
    ///
    /// CPython's generated frozen-dataclass `__setattr__` and `__delattr__` reject all assignments
    /// and deletions on exact instances of the frozen dataclass, but on subclass instances they
    /// only reject assignments and deletions of that dataclass's fields before delegating to the
    /// next method in the MRO.
    fn own_frozen_dataclass_subclass_method(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        method: FrozenDataclassMethod,
    ) -> Option<Type<'db>> {
        if CodeGeneratorKind::from_static_class(db, self).is_some() {
            return None;
        }

        let frozen_base_fields =
            self.inherited_non_slotted_frozen_dataclass_fields(db, specialization, method.name())?;

        let instance_ty = Type::instance(
            db,
            env,
            self.apply_optional_specialization(db, specialization),
        );
        let method_signature = |name_ty, return_ty| {
            let self_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                .with_annotated_type(instance_ty);
            let name_parameter = Parameter::positional_or_keyword(Name::new_static("name"))
                .with_annotated_type(name_ty);
            let parameters = match method {
                FrozenDataclassMethod::SetAttr => Parameters::standard([
                    self_parameter,
                    name_parameter,
                    Parameter::positional_or_keyword(Name::new_static("value")),
                ]),
                FrozenDataclassMethod::DelAttr => {
                    Parameters::standard([self_parameter, name_parameter])
                }
            };
            Signature::new(parameters, return_ty)
        };

        let overloads = frozen_base_fields
            .names
            .iter()
            .map(|field| method_signature(Type::string_literal(db, field), Type::Never))
            .chain([method_signature(
                KnownClass::Str.to_instance(db, env),
                Type::none(db, env),
            )]);

        Some(Type::Callable(CallableType::new(
            db,
            CallableSignature::from_overloads(overloads),
            CallableTypeKind::FunctionLike,
        )))
    }

    /// Determines how an inherited generated frozen-dataclass `method` handles `name`.
    ///
    /// CPython's generated `__setattr__` and `__delattr__` reject every mutation when called on an
    /// instance of the exact frozen class. On an ordinary subclass instance, they reject only
    /// dataclass fields and delegate other names with `super(frozen_class, instance)`.
    ///
    /// If multiple frozen dataclasses are reachable before an explicit implementation of
    /// `method`, a non-field delegates past each generated method.
    /// [`FrozenDataclassDispatch::Delegate`] stores the last frozen base so the caller can perform
    /// the equivalent lookup once, after all of them.
    pub(crate) fn inherited_frozen_dataclass_dispatch(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        method: &str,
        name: &str,
    ) -> Option<FrozenDataclassDispatch<'db>> {
        if CodeGeneratorKind::from_static_class(db, self).is_some()
            || class_member(db, self.body_scope(db), method)
                .ignore_possibly_undefined()
                .is_some()
        {
            return None;
        }

        let frozen_base_fields =
            self.inherited_non_slotted_frozen_dataclass_fields(db, specialization, method)?;

        if frozen_base_fields
            .names
            .iter()
            .any(|field| field.as_str() == name)
        {
            Some(FrozenDataclassDispatch::FrozenField)
        } else {
            Some(FrozenDataclassDispatch::Delegate(
                frozen_base_fields.last_frozen_base,
            ))
        }
    }

    /// Returns the inherited fields whose generated `__setattr__` or `__delattr__` still applies.
    fn inherited_non_slotted_frozen_dataclass_fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        method: &str,
    ) -> Option<InheritedFrozenDataclassFields<'db>> {
        let mut names = FxIndexSet::default();
        let mut last_frozen_base = None;

        for base in self.iter_mro(db, specialization).skip(1) {
            let Some(base_class_type) = base.into_class() else {
                break;
            };
            let Some((base_class, base_specialization)) = base_class_type.static_class_literal(db)
            else {
                break;
            };

            // Stop if another class in the MRO replaces the relevant generated frozen method:
            //
            //   @dataclass(frozen=True)
            //   class Frozen: x: int
            //
            //   class Mutable(Frozen):
            //       def __setattr__(self, name: str, value: object) -> None: ...
            //       def __delattr__(self, name: str) -> None: ...
            //
            //   class Child(Mutable): ...
            //
            // Writes and deletions of `Child().x` dispatch to the corresponding `Mutable` method,
            // not to the synthesized `Frozen` method.
            if class_member(db, base_class.body_scope(db), method)
                .ignore_possibly_undefined()
                .is_some()
            {
                break;
            }

            if base_class.is_frozen_dataclass(db) == Some(true) {
                let field_policy @ CodeGeneratorKind::DataclassLike(_) =
                    CodeGeneratorKind::from_static_class(db, base_class)?
                else {
                    break;
                };

                if base_class.has_dataclass_param(db, field_policy, DataclassFlags::SLOTS) {
                    break;
                }

                names.extend(
                    base_class
                        .fields(db, base_specialization, field_policy)
                        .iter()
                        .filter(|(_, field)| {
                            !matches!(
                                field.kind,
                                FieldKind::Dataclass {
                                    init_only: true,
                                    ..
                                }
                            )
                        })
                        .map(|(name, _)| name.clone()),
                );
                last_frozen_base = Some(base_class);
            }
        }

        Some(InheritedFrozenDataclassFields {
            names: names.into_iter().collect(),
            last_frozen_base: last_frozen_base?,
        })
    }

    /// Member lookup for classes that inherit from `typing.TypedDict`.
    ///
    /// This is implemented as a separate method because the item definitions on a `TypedDict`-based
    /// class are *not* accessible as class members. Instead, this mostly defers to `TypedDictFallback`,
    /// unless `name` corresponds to one of the specialized synthetic members like `__getitem__`.
    pub(crate) fn typed_dict_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        if let Some(member) = self.own_synthesized_member(db, env, specialization, None, name) {
            Place::bound(member).into()
        } else {
            let class = self.apply_optional_specialization(db, specialization);
            let Some(module) = self.typed_dict_module(db) else {
                return Place::Undefined.into();
            };
            typed_dict_class_member(db, env, class, module, policy, name)
        }
    }

    /// Returns a list of all annotated attributes defined in this class, or any of its superclasses.
    ///
    /// See [`StaticClassLiteral::own_fields`] for more details.
    pub(crate) fn fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> &'db FxIndexMap<Name, Field<'db>> {
        if field_policy == CodeGeneratorKind::NamedTuple {
            // NamedTuples do not allow multiple inheritance, so it is sufficient to enumerate the
            // fields of this class only.
            return self.own_fields(db, specialization, field_policy);
        }

        self.fields_inner(db, specialization, field_policy)
    }

    #[salsa::tracked(
        returns(ref),
        cycle_initial=|_, _, _, _, _| FxIndexMap::default(),
        heap_size=get_size2::GetSize::get_heap_size
    )]
    fn fields_inner(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> FxIndexMap<Name, Field<'db>> {
        enum FieldSource<'db> {
            Static(StaticClassLiteral<'db>, Option<Specialization<'db>>),
            DynamicTypedDict(DynamicTypedDictLiteral<'db>),
        }

        debug_assert_ne!(
            field_policy,
            CodeGeneratorKind::NamedTuple,
            "Collecting `fields` for NamedTuples should short-circuit in `fields()`"
        );

        let mut class_variables = FxIndexSet::default();
        let mut map: FxIndexMap<_, _> = self
            .iter_mro(db, specialization)
            .rev()
            .filter_map(|superclass| {
                let class = superclass.into_class()?;

                if let Some((class_literal, specialization)) = class.static_class_literal(db) {
                    // Pydantic collects annotated attributes from every class in the model's MRO,
                    // including ordinary classes that are not themselves Pydantic models.
                    if field_policy.is_pydantic() || field_policy.matches(db, class_literal.into())
                    {
                        return Some(FieldSource::Static(class_literal, specialization));
                    }
                }

                if field_policy == CodeGeneratorKind::TypedDict
                    && let ClassLiteral::DynamicTypedDict(typeddict) = class.class_literal(db)
                {
                    return Some(FieldSource::DynamicTypedDict(typeddict));
                }

                None
            })
            .flat_map(|source| match source {
                FieldSource::Static(class, specialization) => {
                    let own_fields =
                        class.own_fields_with_class_variables(db, specialization, field_policy);

                    if field_policy.is_dataclass_like() {
                        class_variables.extend(own_fields.class_variables.iter().cloned());
                        for name in own_fields.fields.keys() {
                            class_variables.swap_remove(name);
                        }
                    }

                    Either::Left(
                        own_fields
                            .fields
                            .iter()
                            .map(|(name, field)| (name.clone(), field.clone())),
                    )
                }
                FieldSource::DynamicTypedDict(typeddict) => {
                    Either::Right(typeddict.items(db).iter().map(|(name, td_field)| {
                        (
                            name.clone(),
                            Field {
                                declared_ty: td_field.declared_ty,
                                kind: FieldKind::TypedDict {
                                    is_required: td_field.is_required(),
                                    is_read_only: td_field.is_read_only(),
                                },
                                first_declaration: td_field.first_declaration(),
                            },
                        )
                    }))
                }
            })
            // KW_ONLY sentinels are markers, not real fields. Exclude them so
            // they cannot shadow an inherited field with the same name.
            .filter(|(_, field)| !field.is_kw_only_sentinel(db))
            // We collect into a FxOrderMap here to deduplicate attributes
            .collect();

        if field_policy.is_dataclass_like() {
            // `own_fields` excludes class variables, but their declarations can still mask
            // inherited fields. Delay removal so restoring a field preserves its original slot.
            map.retain(|name, _| !class_variables.contains(name));
        }

        map.shrink_to_fit();
        map
    }

    pub(crate) fn validate_members(
        self,
        context: &InferContext<'db, '_>,
        field_policy: CodeGeneratorKind<'db>,
    ) {
        let db = context.db();
        let env = context.program_environment();
        let class_body_scope = self.body_scope(db);
        let table = place_table(db, class_body_scope);
        let use_def = use_def_map(db, class_body_scope);
        for (symbol_id, declarations) in use_def.all_end_of_scope_symbol_declarations() {
            let result = place_from_declarations(db, env, declarations.clone());
            let attr = result.ignore_conflicting_declarations();
            let symbol = table.symbol(symbol_id);
            let name = symbol.name();

            let Some(Type::FunctionLiteral(literal)) = attr.place.ignore_possibly_undefined()
            else {
                continue;
            };

            match name.as_str() {
                "__setattr__" | "__delattr__" => {
                    if field_policy.is_dataclass_like()
                        && self.is_frozen_dataclass(db) == Some(true)
                    {
                        if let Some(builder) = context.report_lint(
                            &INVALID_DATACLASS_OVERRIDE,
                            literal.node(db, context.file(), context.module()),
                        ) {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Cannot overwrite attribute `{}` in frozen dataclass `{}`",
                                name,
                                self.name(db)
                            ));
                            diagnostic.info(name);
                        }
                    }
                }
                "__lt__" | "__le__" | "__gt__" | "__ge__" => {
                    if field_policy.is_dataclass_like()
                        && self.has_dataclass_param(db, field_policy, DataclassFlags::ORDER)
                    {
                        if let Some(builder) = context.report_lint(
                            &INVALID_DATACLASS_OVERRIDE,
                            literal.node(db, context.file(), context.module()),
                        ) {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Cannot overwrite attribute `{}` in dataclass `{}` with `order=True`",
                                name,
                                self.name(db)
                            ));
                            diagnostic.info(name);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Returns a map of all annotated attributes defined in the body of this class.
    /// This extends the `__annotations__` attribute at runtime by also including default values
    /// and computed field properties.
    ///
    /// For a class body like
    /// ```py
    /// @dataclass(kw_only=True)
    /// class C:
    ///     x: int
    ///     y: str = "hello"
    ///     z: float = field(kw_only=False, default=1.0)
    /// ```
    /// we return a map `{"x": Field, "y": Field, "z": Field}` in class-body declaration order,
    /// where each `Field` contains the annotated type, default value (if any), and field
    /// properties.
    ///
    /// **Important**: The returned `Field` objects represent our full understanding of the fields,
    /// including properties inherited from class-level dataclass parameters (like `kw_only=True`)
    /// and dataclass-transform parameters (like `kw_only_default=True`). They do not represent
    /// only what is explicitly specified in each field definition.
    pub(crate) fn own_fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> &'db FxIndexMap<Name, Field<'db>> {
        &self
            .own_fields_with_class_variables(db, specialization, field_policy)
            .fields
    }

    fn own_fields_with_class_variables(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> &'db OwnClassFields<'db> {
        self.own_fields_inner(db, specialization, field_policy)
    }

    /// Collects ordered constructor fields and `ClassVar` masks in one pass over a class body.
    ///
    /// Keeping both together avoids reinterpreting declarations while merging inherited fields.
    #[salsa::tracked(
        attempt = ReturnOnly,
        returns(ref),
        cycle_initial=|_, _, _, _, _| OwnClassFields::default(),
        heap_size=get_size2::GetSize::get_heap_size
    )]
    fn own_fields_inner(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> OwnClassFields<'db> {
        let class_body_scope = self.body_scope(db);
        let env = ProgramEnvironment::from_scope(class_body_scope);
        let table = place_table(db, class_body_scope);

        let use_def = use_def_map(db, class_body_scope);

        // `own_fields(..., NamedTuple)` is called while constructing the class's MRO because the
        // field types determine the synthesized tuple base. `typed_dict_params` also queries the
        // class's MRO, so only read the `total` default when collecting `TypedDict` fields.
        let typed_dict_fields_are_required_by_default =
            if field_policy == CodeGeneratorKind::TypedDict {
                self.typed_dict_params(db)
                    .expect("TypedDictParams should be available for CodeGeneratorKind::TypedDict")
                    .contains(TypedDictParams::TOTAL)
            } else {
                false
            };
        let dataclass_kw_only_default = field_policy.is_dataclass_like().then(|| {
            let own_field_policy =
                CodeGeneratorKind::from_class(db, self.into()).unwrap_or(field_policy);
            self.has_dataclass_param(db, own_field_policy, DataclassFlags::KW_ONLY)
        });
        let mut kw_only_sentinel_field_seen = false;
        let mut field_declarations = Vec::new();

        for (symbol_id, declarations) in use_def.all_end_of_scope_symbol_declarations() {
            // Here, we exclude all declarations that are not annotated assignments. We need this because
            // things like function definitions and nested classes would otherwise be considered dataclass
            // fields. The check is too broad in the sense that it also excludes (weird) constructs where
            // a symbol would have multiple declarations, one of which is an annotated assignment. If we
            // want to improve this, we could instead pass a definition-kind filter to the use-def map
            // query, or to the `symbol_from_declarations` call below. Doing so would potentially require
            // us to generate a union of `__init__` methods.
            if declarations.clone().any_reachable(db, |declaration| {
                declaration.is_defined_and(|declaration| {
                    !matches!(
                        declaration.kind(db),
                        DefinitionKind::AnnotatedAssignment(..)
                    )
                })
            }) {
                continue;
            }

            // Field contents come from the declarations live at end of scope, but field order is
            // anchored to the first reachable annotated declaration in the class body.
            let Some(first_declaration_order) = use_def
                .reachable_symbol_declarations(symbol_id)
                .first_reachable_declaration_order(db, |declaration| {
                    declaration.is_defined_and(|declaration| {
                        matches!(
                            declaration.kind(db),
                            DefinitionKind::AnnotatedAssignment(..)
                        )
                    })
                })
            else {
                continue;
            };

            let result = place_from_declarations(db, &env, declarations.clone());
            field_declarations.push((first_declaration_order, symbol_id, result));
        }

        field_declarations
            .sort_unstable_by_key(|(first_declaration_order, _, _)| *first_declaration_order);

        let mut attributes = FxIndexMap::default();
        let mut class_variables = Vec::new();
        for (_, symbol_id, result) in field_declarations {
            let symbol = table.symbol(symbol_id);
            let first_declaration = result.first_declaration;
            let attr = result.ignore_conflicting_declarations();
            if attr.is_class_var() {
                if field_policy.is_dataclass_like() {
                    class_variables.push(symbol.name().clone());
                }
                continue;
            }

            if let Some(attr_ty) = attr.place.ignore_possibly_undefined() {
                // Annotation-only declarations in stubs also act as bindings for attribute
                // lookup, but they do not supply field defaults.
                let mut default_ty = if field_policy == CodeGeneratorKind::TypedDict
                    || (self.file(db).is_stub(db)
                        && !first_declaration.is_some_and(|definition| {
                            matches!(
                                definition.kind(db),
                                DefinitionKind::AnnotatedAssignment(annotation)
                                    if annotation.has_value()
                            )
                        })) {
                    None
                } else {
                    place_from_bindings(db, &env, use_def.end_of_scope_symbol_bindings(symbol_id))
                        .place
                        .ignore_possibly_undefined()
                };

                default_ty =
                    default_ty.map(|ty| ty.apply_optional_specialization(db, specialization));

                let mut init = true;
                let mut kw_only = None;
                let mut alias = None;
                let mut converter = None;
                let mut strict = pydantic::ConfigBoolean::Unspecified;
                if field_policy.is_pydantic() {
                    let metadata =
                        pydantic::field_metadata(db, first_declaration, default_ty, specialization);
                    default_ty = metadata.default_ty;
                    init = metadata.init;
                    alias = metadata.alias;
                    strict = metadata.strict;
                } else if let Some(Type::KnownInstance(KnownInstanceType::Field(field))) =
                    default_ty
                {
                    default_ty = field.default_type(db);
                    init = field.init(db);
                    kw_only = field.kw_only(db);
                    alias.clone_from(field.alias(db));
                    converter = field.converter(db);
                }

                let kind = match field_policy {
                    CodeGeneratorKind::NamedTuple => FieldKind::NamedTuple { default_ty },
                    CodeGeneratorKind::DataclassLike(_) => FieldKind::Dataclass {
                        default_ty,
                        init_only: attr.is_init_var(),
                        init,
                        kw_only,
                        alias,
                        converter,
                    },
                    CodeGeneratorKind::Pydantic(_) => FieldKind::Pydantic {
                        default_ty,
                        // Private attributes are instance attributes but never constructor parameters.
                        init: init && !pydantic::is_private_attribute(symbol.name()),
                        alias,
                        strict,
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
                            typed_dict_fields_are_required_by_default
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
                if field_policy.is_dataclass_like() && field.is_kw_only_sentinel(db) {
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

                // Resolve the kw_only to the class-level default. This ensures that when fields
                // are inherited by child classes, they use their defining class's kw_only default.
                if let FieldKind::Dataclass {
                    kw_only: ref mut kw @ None,
                    ..
                } = field.kind
                {
                    *kw = dataclass_kw_only_default;
                }

                attributes.insert(symbol.name().clone(), field);
            }
        }

        attributes.shrink_to_fit();

        OwnClassFields {
            fields: attributes,
            class_variables: class_variables.into_boxed_slice(),
        }
    }

    /// Return the type qualifiers attached to each reachable annotated assignment in source order.
    ///
    /// This uses the declaration history rather than [`StaticClassLiteral::own_fields`], because a
    /// later method or nested class can replace the symbol's binding while leaving its entry in
    /// `__annotations__`:
    ///
    /// ```python
    /// class Example(NamedTuple):
    ///     value: Final[int]
    ///     def value(self) -> int: ...
    /// ```
    ///
    /// Each qualifier remains paired with its own definition so diagnostics can point to the
    /// annotation that introduced it, including when declarations occur in different branches.
    pub(crate) fn own_annotated_qualifiers(
        self,
        db: &'db dyn Db,
    ) -> Vec<(Name, TypeQualifiers, Definition<'db>)> {
        let body_scope = self.body_scope(db);
        let table = place_table(db, body_scope);
        let use_def = use_def_map(db, body_scope);
        let mut annotated_qualifiers = Vec::new();

        for (symbol_id, _) in use_def.all_end_of_scope_symbol_declarations() {
            let declarations = use_def.reachable_symbol_declarations(symbol_id);
            let predicates = declarations.predicates();
            let reachability_constraints = declarations.reachability_constraints();

            for declaration in declarations {
                if reachability_constraints
                    .evaluate(db, predicates, declaration.reachability_constraint)
                    .is_always_false()
                {
                    continue;
                }

                let DefinitionState::Defined(definition) = declaration.declaration else {
                    continue;
                };
                if !matches!(definition.kind(db), DefinitionKind::AnnotatedAssignment(..)) {
                    continue;
                }

                let Some(declared) = inferred_declaration(db, definition).declared() else {
                    continue;
                };
                annotated_qualifiers.push((
                    declaration.declaration_order,
                    table.symbol(symbol_id).name().clone(),
                    declared.qualifiers(),
                    definition,
                ));
            }
        }

        annotated_qualifiers
            .sort_unstable_by_key(|(declaration_order, _, _, _)| *declaration_order);
        annotated_qualifiers
            .into_iter()
            .map(|(_, name, qualifiers, definition)| (name, qualifiers, definition))
            .collect()
    }

    /// Look up an instance attribute (available in `__dict__`) of the given name.
    ///
    /// See [`Type::instance_member`] for more details.
    pub(super) fn instance_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        match static_instance_member_sync(
            env,
            self,
            specialization,
            name,
            &InlineInstanceStorageEffects::new(db),
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// A helper function for `instance_member` that looks up the `name` attribute only on
    /// this class, not on its superclasses.
    pub(super) fn own_instance_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Member<'db> {
        match static_own_instance_member_sync(
            env,
            self,
            name,
            &InlineMemberSourceEffects::new(db),
        ) {
            Ok(member) => member,
            Err(never) => match never {},
        }
    }

    /// Returns `true` if `name` is a non-init-only field directly declared on this
    /// dataclass (i.e., a field that corresponds to an instance attribute).
    ///
    /// This is used to decide whether a bare class-body annotation like `x: int`
    /// should be treated as defining an instance attribute: dataclass fields are
    /// implicitly assigned in `__init__`, so they behave as instance attributes
    /// even though no explicit binding exists in the class body.
    fn is_own_dataclass_instance_field(self, db: &'db dyn Db, name: &str) -> bool {
        let Some(field_policy) =
            instance_field_policy(CodeGeneratorKind::from_static_class(db, self))
        else {
            return false;
        };

        let fields = self.own_fields(db, None, field_policy);
        let Some(field) = fields.get(name) else {
            return false;
        };
        matches!(
            field.kind,
            FieldKind::Dataclass {
                init_only: false,
                ..
            } | FieldKind::Pydantic { .. }
        )
    }

    /// Returns the converter's input type (i.e., the type of its first positional parameter) for a
    /// dataclass field, if the field has a converter function specified.
    pub(super) fn converter_input_type_for_field(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> Option<Type<'db>> {
        let field_policy @ CodeGeneratorKind::DataclassLike(_) =
            CodeGeneratorKind::from_static_class(db, self)?
        else {
            return None;
        };
        let fields = self.fields(db, None, field_policy);
        let field = fields.get(name)?;
        if let FieldKind::Dataclass { converter, .. } = field.kind {
            converter.map(|(input_ty, _)| input_ty)
        } else {
            None
        }
    }

    pub(super) fn to_non_generic_instance(self, db: &'db dyn Db) -> Type<'db> {
        let env = ProgramEnvironment::from_scope(self.body_scope(db));
        Type::instance(db, &env, ClassType::NonGeneric(self.into()))
    }

    /// Return this class' involvement in an inheritance cycle, if any.
    ///
    /// A class definition like this will fail at runtime,
    /// but we must be resilient to it or we could panic.
    pub(crate) fn inheritance_cycle(self, db: &'db dyn Db) -> Option<InheritanceCycle> {
        match inheritance_cycle_sync(self, &InlineInheritanceCycle(db)) {
            Ok(cycle) => cycle,
            Err(never) => match never {},
        }
    }

    /// Returns a [`Span`] with the range of the class's header.
    ///
    /// See [`Self::header_range`] for more details.
    pub(crate) fn header_span(self, db: &'db dyn Db) -> Span {
        Span::from(self.file(db)).with_range(self.header_range(db))
    }

    /// Returns the range of the class's "header": the class name
    /// and any arguments passed to the `class` statement. E.g.
    ///
    /// ```ignore
    /// class Foo(Bar, metaclass=Baz): ...
    ///       ^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let class_scope = self.body_scope(db);
        let module = parsed_module(db, class_scope.python_file(db)).load(db);
        let class_node = self.node(db, &module);
        Self::header_range_from_node(class_node)
    }

    pub(in crate::types) fn header_range_from_node(class_node: &ast::StmtClassDef) -> TextRange {
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

    /// Returns the range of the class's name
    pub(crate) fn focus_range(self, db: &'db dyn Db) -> TextRange {
        let class_scope = self.body_scope(db);
        let module = parsed_module(db, class_scope.python_file(db)).load(db);
        let class_node = self.node(db, &module);
        class_node.name.range()
    }
}

/// A single semantic class-base entry after expanding starred tuple bases.
#[derive(Clone, Copy)]
pub(crate) struct ExpandedClassBaseEntry<'a, 'db> {
    pub(super) source_node: &'a ast::Expr,
    pub(super) ty: Type<'db>,
}

impl<'a, 'db> ExpandedClassBaseEntry<'a, 'db> {
    /// Returns the source expression for this base entry.
    pub(crate) const fn source_node(self) -> &'a ast::Expr {
        self.source_node
    }

    /// Returns the semantic type of this base entry.
    pub(crate) const fn ty(self) -> Type<'db> {
        self.ty
    }
}

/// Expands a class's bases into the semantic entries used by [`StaticClassLiteral::explicit_bases`].
pub(crate) fn expanded_class_base_entries<'a, 'db>(
    db: &'db dyn Db,
    known_class: Option<KnownClass>,
    class_stmt: &'a ast::StmtClassDef,
    class_definition: Definition<'db>,
) -> Vec<ExpandedClassBaseEntry<'a, 'db>> {
    match expanded_class_base_entries_with(
        known_class,
        class_stmt,
        class_definition,
        &InlineClassBaseEntryEffects::new(db),
    ) {
        Ok(entries) => entries,
        Err(never) => match never {},
    }
}

fn explicit_base_types_with<'db, E>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<Box<[Type<'db>]>, <E as ClassBaseEntryEffects<'db>>::Error>
where
    E: ClassBaseEntryEffects<'db>
        + SourceReadControl<Error = <E as ClassBaseEntryEffects<'db>>::Error>,
{
    explicit_base_types_sync(
        class,
        ExplicitBaseFacts,
        &InlineExplicitBaseEffects::new(db, effects),
    )
}

impl<'db> VarianceInferable<'db> for StaticClassLiteral<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        _: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        VarianceTerm::variable(db, VarianceOrigin::Class(self), typevar)
    }
}

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    /// Build a definition-site equation before substituting type arguments. Supported protocols
    /// use their structural interface; `TypedDict` classes use their fields. Other classes retain the
    /// ordinary attribute and base-class variance rules.
    #[salsa::tracked(attempt = ReturnOnly, returns(copy), cycle_initial=|_, _, _, _| VarianceTerm::BIVARIANT, heap_size=ruff_memory_usage::heap_size)]
    pub(in crate::types) fn variance_equation(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let env = ProgramEnvironment::from_scope(self.body_scope(db));

        if self.is_typed_dict(db) {
            return TypedDictType::new(self.identity_specialization(db))
                .variance_of_items(db, &env, typevar);
        }

        let typevar_in_generic_context = self
            .generic_context(db)
            .is_some_and(|generic_context| generic_context.contains(db, typevar));

        if !typevar_in_generic_context {
            return VarianceTerm::BIVARIANT;
        }

        if self.is_protocol(db)
            && let Some(protocol) = self.identity_specialization(db).into_protocol_class(db)
            && protocol.supports_variance_inference(db)
        {
            return protocol.interface(db).variance_of(db, &env, typevar);
        }

        let class_body_scope = self.body_scope(db);
        let program_file = class_body_scope.program_file(db);
        let python_version = env.python_version(db);

        let index = semantic_index(db, program_file);

        let explicit_bases_variances = self
            .explicit_bases(db)
            .iter()
            .map(|class| class.variance_of(db, &env, typevar));

        let default_attribute_variance = {
            let is_namedtuple = CodeGeneratorKind::NamedTuple.matches(db, self.into());
            // Python 3.13 introduced a synthesized `__replace__` method on dataclasses which uses
            // their field types in contravariant position, thus meaning a frozen dataclass must
            // still be invariant in its field types. Other synthesized methods on dataclasses are
            // not considered here, since they don't use field types in their signatures. TODO:
            // ideally we'd have a single source of truth for information about synthesized
            // methods, so we just look them up normally and don't hardcode this knowledge here.
            let is_frozen_dataclass_prior_to_313 = python_version <= PythonVersion::PY312
                && CodeGeneratorKind::from_static_class(db, self)
                    .is_some_and(|kind| self.has_dataclass_param(db, kind, DataclassFlags::FROZEN));

            if is_namedtuple || is_frozen_dataclass_prior_to_313 {
                TypeVarVariance::Covariant
            } else {
                TypeVarVariance::Invariant
            }
        };

        let init_name: &Name = &"__init__".into();
        let new_name: &Name = &"__new__".into();

        let use_def_map = index.use_def_map(class_body_scope.file_scope_id(db));
        let table = place_table(db, class_body_scope);
        // A declaration in a stub also creates a binding with no qualifiers. Resolve
        // both together so `value: Final[T]` is not also treated as a mutable `T`.
        let attribute_places_and_qualifiers = use_def_map
            .all_end_of_scope_symbol_declarations()
            .filter_map(|(symbol_id, _)| {
                let name = table.symbol(symbol_id).name();
                if [init_name, new_name].contains(&name) {
                    return None;
                }

                let place_and_qualifiers = place_by_id(
                    db,
                    class_body_scope,
                    symbol_id.into(),
                    RequiresExplicitReExport::No,
                    ConsideredDefinitions::EndOfScope,
                );
                Some((name.to_string(), place_and_qualifiers, true))
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

        let receiver = self.variance_receiver(db, &env);
        let attribute_variances = attribute_names
            .map(|name| {
                let place_and_quals = self.own_instance_member(db, &env, &name).inner;
                (name, place_and_quals, false)
            })
            .chain(attribute_places_and_qualifiers)
            .dedup()
            .filter_map(|(name, place_and_qual, is_class_member)| {
                place_and_qual.ignore_possibly_undefined().map(|ty| {
                    let variance = if place_and_qual
                        .qualifiers
                        // None of these fields can be mutated through an instance.
                        .intersects(
                            TypeQualifiers::CLASS_VAR
                                | TypeQualifiers::FINAL
                                | TypeQualifiers::READ_ONLY,
                        )
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
                        // FINAL and READ_ONLY: immutable fields are covariant.
                        TypeVarVariance::Covariant
                    } else {
                        default_attribute_variance
                    };
                    if !is_class_member {
                        return ty.with_polarity(variance).variance_of(db, &env, typevar);
                    }

                    if let Type::PropertyInstance(property) = ty {
                        // A property subclass can also expose mutable state on the descriptor
                        // itself, independently of its getter and setter.
                        let instance_variance = property
                            .instance_fallback(db, &env)
                            .variance_of(db, &env, typevar);
                        let accessor_variances = [
                            property.getter(db),
                            property.setter(db),
                            property.deleter(db),
                        ]
                        .into_iter()
                        .flatten()
                        .map(|accessor| {
                            MemberVariance::accessor(db, &env, accessor, receiver)
                                .variance_of(db, &env, typevar)
                        });
                        return VarianceTerm::join(
                            db,
                            std::iter::once(instance_variance).chain(accessor_variances),
                        );
                    }
                    let member = MemberVariance::of(db, &env, ty, receiver);
                    let exposed_variance = member.variance_of(db, &env, typevar);
                    match member.write_domain {
                        DescriptorSetterDomain::Known(_) => exposed_variance,
                        // Keep the ordinary attribute contribution when a descriptor's write
                        // domain cannot be represented, without dropping the known read type.
                        DescriptorSetterDomain::Deferred => VarianceTerm::join(
                            db,
                            [
                                exposed_variance,
                                ty.with_polarity(variance).variance_of(db, &env, typevar),
                            ],
                        ),
                        DescriptorSetterDomain::Missing => {
                            VarianceTerm::from(variance).compose_thunk(db, || exposed_variance)
                        }
                    }
                })
            });

        VarianceTerm::join(db, attribute_variances.chain(explicit_bases_variances))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) enum InheritanceCycle {
    /// The class is cyclically defined and is a participant in the cycle.
    /// i.e., it inherits either directly or indirectly from itself.
    Participant,
    /// The class inherits from a class that is a `Participant` in an inheritance cycle,
    /// but is not itself a participant.
    Inherited,
}

impl InheritanceCycle {
    pub(crate) const fn is_participant(self) -> bool {
        matches!(self, InheritanceCycle::Participant)
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousInheritanceCycleEffects)]
    pub(in crate::types) trait InheritanceCycleEffects<'db> {
        type Error;

        #[operation(source)]
        async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

        #[operation(child)]
        async fn inheritance_cycle(&self, class: StaticClassLiteral<'db>) -> Result<Option<InheritanceCycle>, Self::Error>;
    }

    #[synchronous(inheritance_cycle_sync)]
    #[capabilities(effects = InheritanceCycleEffects)]
    #[passive_values()]
    pub(in crate::types) async fn inheritance_cycle_with<'db, E: InheritanceCycleEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<Option<InheritanceCycle>, E::Error> {
        if !effects.has_explicit_bases(class).await? {
            return Ok(None);
        }
        effects.inheritance_cycle(class).await
    }
}

struct InlineInheritanceCycle<'db>(&'db dyn Db);

impl<'db> SynchronousInheritanceCycleEffects<'db> for InlineInheritanceCycle<'db> {
    type Error = Infallible;

    fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.has_explicit_bases(self.0))
    }

    fn inheritance_cycle(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<InheritanceCycle>, Infallible> {
        Ok(inheritance_cycle_inner(self.0, class))
    }
}

fn explicit_bases_cycle_initial<'db>(
    db: &'db dyn Db,
    id: salsa::Id,
    literal: StaticClassLiteral<'db>,
) -> Box<[Type<'db>]> {
    initial_explicit_base_types_sync(
        id,
        literal,
        &InlineExplicitBaseEffects::new(db, &SourceClassEffects::new(db)),
    )
    .unwrap_or_default()
}

fn explicit_bases_cycle_fn<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &[Type<'db>],
    current: Box<[Type<'db>]>,
    literal: StaticClassLiteral<'db>,
) -> Box<[Type<'db>]> {
    // The synchronous recovery adapter retains `current` when continuation is interrupted.
    recover_explicit_base_types_sync(
        cycle,
        previous,
        current,
        literal,
        ExplicitBaseFacts,
        &InlineExplicitBaseEffects::new(db, &SourceClassEffects::new(db)),
    )
    .unwrap_or_default()
}

impl<'db> SynchronousStaticInstanceMemberEffects<'db> for InlineMemberSourceEffects<'db> {
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Self::Error> {
        Ok(class.body_scope(self.db))
    }
    fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Infallible> {
        Ok(CodeGeneratorKind::from_static_class(self.db, class))
    }
    fn has_own_named_tuple_field(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Infallible> {
        Ok(class
            .own_fields(self.db, None, CodeGeneratorKind::NamedTuple)
            .contains_key(name))
    }
    fn declaration_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Infallible> {
        Ok(place_from_declarations(self.db, env, declarations))
    }
    fn imported_final<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        result: PlaceFromDeclarationsResult<'db>,
        imported: ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Infallible> {
        Ok(result.with_imported_final(self.db, env, imported))
    }
    fn implicit_member(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Infallible> {
        Ok(class.implicit_attribute(self.db, name, MethodDecorator::None))
    }
    fn is_kw_only(&self, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(ty.is_instance_of(self.db, KnownClass::KwOnly))
    }
    fn is_stub(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.file(self.db).is_stub(self.db))
    }
    fn has_instance_slot(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Infallible> {
        Ok(class.has_instance_slot(self.db, name))
    }
    fn is_own_dataclass_instance_field(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Infallible> {
        Ok(class.is_own_dataclass_instance_field(self.db, name))
    }
    fn getter_member(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(ty.class_member(self.db, env, "__get__"))
    }
    fn union_two(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(UnionType::from_two_elements(self.db, env, first, second))
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) DecoratorsInnerConfiguration), attempt = ReturnOnly, self_ty = StaticClassLiteral<'db>, returns(deref), cycle_initial=|_, _, _| Box::default(), heap_size=ruff_memory_usage::heap_size)]
fn decorators_inner_<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> Box<[Type<'db>]> {
    tracing::trace!("StaticClassLiteral::decorators: {}", class.name(db));

    match class_decorators_sync(class, DecoratorFacts, &InlineClassDecoratorEffects(db)) {
        Ok(decorators) => decorators,
        Err(error) => match error {},
    }
}

pub(in crate::types) fn class_decorators_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<DecoratorsInnerConfiguration> {
    decorators_inner_::fn_ingredient_(db, db.zalsa())
}

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousStaticFinalityEffects)]
pub(in crate::types) trait StaticFinalityEffects<'db> {
    type Error;
    #[operation(checkpoint)]
    async fn checkpoint(&self) -> Result<(), Self::Error>;
    #[operation(source)]
    async fn has_decorators(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    #[operation(source)]
    async fn has_final_decorator(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    #[operation(source)]
    async fn has_enum_metadata(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
}

#[synchronous(static_finality_sync)]
#[capabilities(effects = StaticFinalityEffects)]
#[passive_values()]
pub(in crate::types) async fn static_finality_with<'db, E: StaticFinalityEffects<'db>>(
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.checkpoint().await?;
    if effects.has_decorators(class).await? && effects.has_final_decorator(class).await? {
        Ok(true)
    } else {
        effects.has_enum_metadata(class).await
    }
}
}

struct InlineStaticFinality<'db>(&'db dyn Db);

impl<'db> SynchronousStaticFinalityEffects<'db> for InlineStaticFinality<'db> {
    type Error = std::convert::Infallible;
    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn has_decorators(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.has_decorators(self.0))
    }
    fn has_final_decorator(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        has_known_class_decorator_sync(
            class,
            KnownFunction::Final,
            DecoratorFacts,
            &InlineClassDecoratorEffects(self.0),
        )
    }
    fn has_enum_metadata(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(enum_metadata(self.0, ClassLiteral::Static(class)).is_some())
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) InheritedLegacyGenericContextInnerConfiguration), attempt = ReturnOnly,
    returns(copy),
    cycle_initial=|_, _, _| None,
    heap_size=ruff_memory_usage::heap_size,
)]
fn inherited_legacy_generic_context_inner<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<GenericContext<'db>> {
    super::context::inherited::inherited_context_with(db, class, &SourceClassEffects::new(db))
        .unwrap_or(None)
}

#[salsa::tracked(configuration = (pub(in crate::types) InstanceFlagsInnerConfiguration), attempt = ReturnOnly,
    returns(copy),
    cycle_initial=|_, _, _| ClassInstanceFlags::empty(),
    heap_size=ruff_memory_usage::heap_size,
)]
fn instance_flags_inner<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> ClassInstanceFlags {
    match inherited_instance_flags_with(db, class, &SourceClassEffects::new(db)) {
        Ok(flags) => flags,
        #[cfg(test)]
        Err(_) => ClassInstanceFlags::empty(),
        #[cfg(not(test))]
        Err(never) => match never {},
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn instance_flags_inner_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<InstanceFlagsInnerConfiguration> {
    instance_flags_inner::fn_ingredient_(db, db.zalsa())
}

struct InlineStaticMetaclassEffects<'db>(&'db dyn Db);

impl<'db> SynchronousStaticMetaclassEffects<'db> for InlineStaticMetaclassEffects<'db> {
    type Error = Infallible;

    fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.has_explicit_bases(self.0))
    }

    fn has_explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.has_explicit_metaclass(self.0))
    }

    fn known_class(
        &self,
        class: StaticClassLiteral<'db>,
        known: KnownClass,
    ) -> Result<Type<'db>, Infallible> {
        let env = ProgramEnvironment::from_scope(class.body_scope(self.0));
        Ok(known.to_class_literal(self.0, &env))
    }

    fn try_metaclass_inner(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<MetaclassSelectionResult<'db>, Infallible> {
        Ok(try_metaclass_inner(self.0, class))
    }

    fn try_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<MetaclassSelectionResult<'db>, Infallible> {
        static_try_metaclass_sync(class, self)
    }

    fn inferred_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassMetaclass<'db>, Infallible> {
        static_inferred_metaclass_sync(class, self)
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) TryMetaclassInnerConfiguration), attempt = ReturnOnly,
    returns(clone),
    cycle_initial=|_, _, _| Err(MetaclassError {
        kind: MetaclassErrorKind::Cycle,
    }),
    heap_size=ruff_memory_usage::heap_size,
)]
fn try_metaclass_inner<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Result<(ClassMetaclass<'db>, Option<MetaclassTransformInfo<'db>>), MetaclassError<'db>> {
    tracing::trace!("StaticClassLiteral::try_metaclass: {}", class.name(db));
    match inner_metaclass_sync(class, &OrdinaryInnerMetaclass(db)) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

/// Exposes the existing metaclass query's ingredient for canonical controlled routing.
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn try_metaclass_inner_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<TryMetaclassInnerConfiguration> {
    try_metaclass_inner::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked(configuration = (pub(in crate::types) InheritanceCycleInnerConfiguration), attempt = ReturnOnly, returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
fn inheritance_cycle_inner<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<InheritanceCycle> {
    tracing::trace!("Class::inheritance_cycle: {}", class.name(db));
    match inheritance_cycle_inner_sync(class, &OrdinaryCycleTraversal(db)) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn inheritance_cycle_inner_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<InheritanceCycleInnerConfiguration> {
    inheritance_cycle_inner::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked(configuration = (pub(in crate::types) HasOwnOrderingMethodConfiguration), self_ty = StaticClassLiteral<'db>, returns(copy))]
fn has_own_ordering_method<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> bool {
    let body_scope = class.body_scope(db);
    ["__lt__", "__le__", "__gt__", "__ge__"]
        .iter()
        .any(|method| !class_member(db, body_scope, method).is_undefined())
}

#[salsa::tracked(configuration = (pub(in crate::types) HasOwnComparisonMethodsConfiguration), self_ty = StaticClassLiteral<'db>, attempt = ReturnOnly, returns(copy))]
fn has_own_comparison_methods<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> bool {
    let body_scope = class.body_scope(db);
    ["__lt__", "__le__", "__gt__", "__ge__"]
        .iter()
        .all(|method| !class_member(db, body_scope, method).is_undefined())
}

#[salsa::tracked(configuration = (pub(in crate::types) TypedDictModuleConfiguration), self_ty = StaticClassLiteral<'db>, returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
fn typed_dict_module<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> Option<TypingModule> {
    class
        .iter_mro(db, None)
        .find_map(ClassBase::typed_dict_module)
}

#[cfg(feature = "experimental-analysis")]
crate::types::class::runtime::class_memo_schema! {
    pub(super) type ClassMemoSchema<'db> = crate::types::StaticClassLiteral<'static>;
    pub(super) fn register_class_memos;
    (static_class_generic_context, salsa::execution_probe::FixedQueryKeyProfile),
            (pep695_generic_context_inner, salsa::execution_probe::FixedQueryKeyProfile),
            (inherited_legacy_generic_context_inner, salsa::execution_probe::FixedQueryKeyProfile),
            (instance_flags_inner, salsa::execution_probe::FixedQueryKeyProfile),
            (explicit_bases_inner, crate::types::class::runtime::TypeSliceProfile),
            (try_mro_unspecialized, crate::types::class::runtime::MroProfile),
            (try_metaclass_inner, crate::types::class::runtime::MetaclassProfile),
            (inheritance_cycle_inner, salsa::execution_probe::FixedQueryKeyProfile),
            (has_own_ordering_method, salsa::execution_probe::FixedQueryKeyProfile),
            (has_own_comparison_methods, salsa::execution_probe::FixedQueryKeyProfile),
            (typed_dict_module, salsa::execution_probe::FixedQueryKeyProfile),
            (decorators_inner_, crate::types::class::runtime::TypeSliceProfile)
}
