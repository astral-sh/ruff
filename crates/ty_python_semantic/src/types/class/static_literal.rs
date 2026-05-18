use itertools::{Either, Itertools};
use ruff_db::{
    diagnostic::Span,
    files::File,
    parsed::{ParsedModuleRef, parsed_module},
};
use ruff_python_ast as ast;
use ruff_python_ast::{PythonVersion, name::Name};
use ruff_text_size::{Ranged, TextRange};
use std::cell::RefCell;

use crate::{
    Db, FxIndexMap, FxIndexSet, Program, TypeQualifiers,
    place::{
        DefinedPlace, Definedness, Place, PlaceAndQualifiers, PublicTypePolicy, TypeOrigin,
        place_from_bindings, place_from_declarations,
    },
    reachability::{DeclarationsIteratorExtension, binding_reachability},
    types::{
        ApplyTypeMappingVisitor, BoundTypeVarInstance, CallArguments, CallableType, ClassBase,
        ClassLiteral, ClassType, DATACLASS_FLAGS, DataclassFlags, DataclassParams, GenericAlias,
        GenericContext, KnownClass, KnownInstanceType, MaterializationKind, MemberLookupPolicy,
        MetaclassCandidate, MetaclassTransformInfo, Parameter, Parameters, PropertyInstanceType,
        Signature, SpecialFormType, StaticMroError, SubclassOfType, Truthiness, Type, TypeContext,
        TypeMapping, TypeVarVariance, UnionBuilder, UnionType,
        call::{CallError, CallErrorKind},
        callable::CallableTypeKind,
        class::{
            ClassMemberResult, CodeGeneratorKind, DisjointBase, DynamicTypedDictLiteral, Field,
            FieldKind, InstanceMemberResult, MetaclassError, MetaclassErrorKind, MethodDecorator,
            MroLookup, NamedTupleField, SlotsKind, synthesize_namedtuple_class_member,
            typed_dict::{TypedDictFields, synthesize_typed_dict_method, typed_dict_class_member},
        },
        context::InferContext,
        declaration_type, definition_expression_type, determine_upper_bound,
        diagnostic::INVALID_DATACLASS_OVERRIDE,
        enums::{enum_metadata, is_enum_class_by_inheritance, try_unwrap_nonmember_value},
        function::{
            DataclassTransformerParams, KnownFunction, is_implicit_classmethod,
            is_implicit_staticmethod,
        },
        generics::Specialization,
        infer::infer_unpack_types,
        infer_expression_type,
        known_instance::DeprecatedInstance,
        member::{Member, class_member},
        mro::{Mro, MroIterator},
        signatures::CallableSignature,
        tuple::{FixedLengthTuple, Tuple, TupleSpec, TupleType},
        typed_dict::{TypedDictParams, typed_dict_params_from_class_def},
        variance::VarianceInferable,
        visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard},
    },
};
use crate::{attribute_assignments, attribute_declarations};
use ty_python_core::{
    attribute_scopes,
    definition::{Definition, DefinitionKind, DefinitionState, TargetKind},
    place_table,
    scope::{Scope, ScopeId},
    semantic_index,
    symbol::Symbol,
    use_def_map,
};

/// Representation of a class definition statement in the AST: either a non-generic class, or a
/// generic class that has not been specialized.
///
/// This does not in itself represent a type, but can be transformed into a [`ClassType`] that
/// does. (For generic classes, this requires specializing its generic context.)
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct StaticClassLiteral<'db> {
    /// Name of the class at definition
    #[returns(ref)]
    pub(crate) name: Name,

    pub(crate) body_scope: ScopeId<'db>,

    pub(crate) known: Option<KnownClass>,

    /// If this class is deprecated, this holds the deprecation message.
    pub(crate) deprecated: Option<DeprecatedInstance<'db>>,

    pub(crate) type_check_only: bool,

    pub(crate) dataclass_params: Option<DataclassParams<'db>>,
    pub(crate) dataclass_transformer_params: Option<DataclassTransformerParams<'db>>,

    /// Whether this class is decorated with `@functools.total_ordering`
    pub(crate) total_ordering: bool,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for StaticClassLiteral<'_> {}

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
    pub(crate) fn namedtuple_base_has_unknown_fields(self, db: &'db dyn Db) -> bool {
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
    pub(crate) fn is_dataclass_like(self, db: &'db dyn Db) -> bool {
        matches!(
            CodeGeneratorKind::from_class(db, ClassLiteral::Static(self), None),
            Some(CodeGeneratorKind::DataclassLike(_))
        )
    }

    /// Returns a new [`StaticClassLiteral`] with the given dataclass params, preserving all other fields.
    pub(crate) fn with_dataclass_params(
        self,
        db: &'db dyn Db,
        dataclass_params: Option<DataclassParams<'db>>,
    ) -> Self {
        Self::new(
            db,
            self.name(db).clone(),
            self.body_scope(db),
            self.known(db),
            self.deprecated(db),
            self.type_check_only(db),
            dataclass_params,
            self.dataclass_transformer_params(db),
            self.total_ordering(db),
        )
    }

    /// Returns `true` if this class defines any ordering method (`__lt__`, `__le__`, `__gt__`,
    /// `__ge__`) in its own body (not inherited). Used by `@total_ordering` to determine if
    /// synthesis is valid.
    #[salsa::tracked]
    pub(crate) fn has_own_ordering_method(self, db: &'db dyn Db) -> bool {
        let body_scope = self.body_scope(db);
        ["__lt__", "__le__", "__gt__", "__ge__"]
            .iter()
            .any(|method| !class_member(db, body_scope, method).is_undefined())
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
    pub(super) fn total_ordering_root_method(
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

    #[salsa::tracked(
        cycle_initial=|_, _, _| None,
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

    #[salsa::tracked(
        cycle_initial=|_, _, _| None,
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

    /// Returns all of the typevars that are referenced in this class's base class list.
    /// (This is used to ensure that classes do not reference typevars from enclosing
    /// generic contexts.)
    pub(crate) fn typevars_referenced_in_bases(
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
        for base in self.explicit_bases(db) {
            visitor.visit_type(db, *base);
        }
        visitor.typevars.into_inner()
    }

    /// Returns the generic context that should be inherited by any constructor methods of this class.
    pub(super) fn inherited_generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.generic_context(db)
    }

    pub(crate) fn file(self, db: &dyn Db) -> File {
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
            None => ClassType::NonGeneric(self.into()),
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
    #[salsa::tracked(returns(deref), cycle_initial=explicit_bases_cycle_initial, cycle_fn=explicit_bases_cycle_fn, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn explicit_bases(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        tracing::trace!(
            "StaticClassLiteral::explicit_bases_query: {}",
            self.name(db)
        );

        let module = parsed_module(db, self.file(db)).load(db);
        let class_stmt = self.node(db, &module);

        let class_definition =
            semantic_index(db, self.file(db)).expect_single_definition(class_stmt);

        expanded_class_base_entries(db, self.known(db), class_stmt, class_definition)
            .into_iter()
            .map(ExpandedClassBaseEntry::ty)
            .collect()
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
        } else if SlotsKind::from(db, self) == SlotsKind::NotEmpty {
            Some(DisjointBase::due_to_dunder_slots(ClassLiteral::Static(
                self,
            )))
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
    pub(crate) fn is_protocol(self, db: &'db dyn Db) -> bool {
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

    /// Return the types of the decorators on this class
    #[salsa::tracked(returns(deref), cycle_initial=|_, _, _| Box::default(), heap_size=ruff_memory_usage::heap_size)]
    fn decorators(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        tracing::trace!("StaticClassLiteral::decorators: {}", self.name(db));

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

    pub(crate) fn known_function_decorators(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = KnownFunction> + 'db {
        self.decorators(db)
            .iter()
            .filter_map(|deco| deco.as_function_literal())
            .filter_map(|decorator| decorator.known(db))
    }

    /// Iterate through the decorators on this class, returning the position of the first one
    /// that matches the given predicate.
    pub(super) fn find_decorator_position(
        self,
        db: &'db dyn Db,
        predicate: impl Fn(Type<'db>) -> bool,
    ) -> Option<usize> {
        self.decorators(db)
            .iter()
            .position(|decorator| predicate(*decorator))
    }

    /// Iterate through the decorators on this class, returning the index of the first one
    /// that is either `@dataclass` or `@dataclass(...)`.
    pub(crate) fn find_dataclass_decorator_position(self, db: &'db dyn Db) -> Option<usize> {
        self.find_decorator_position(db, |ty| match ty {
            Type::FunctionLiteral(function) => function.is_known(db, KnownFunction::Dataclass),
            Type::DataclassDecorator(_) => true,
            _ => false,
        })
    }

    /// Is this class final?
    pub(crate) fn is_final(self, db: &'db dyn Db) -> bool {
        self.known_function_decorators(db)
            .contains(&KnownFunction::Final)
            || enum_metadata(db, ClassLiteral::Static(self)).is_some()
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
    #[salsa::tracked(
        returns(as_ref),
        cycle_initial=|db, _, self_: StaticClassLiteral<'db>, specialization| {
            Err(StaticMroError::cycle(
                db,
                self_.apply_optional_specialization(db, specialization),
            ))
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(crate) fn try_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, StaticMroError<'db>> {
        tracing::trace!("StaticClassLiteral::try_mro: {}", self.name(db));
        Mro::of_static_class(db, self, specialization)
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

    /// Return `true` if this class constitutes a typed dict specification (inherits from
    /// `typing.TypedDict`, either directly or indirectly).
    #[salsa::tracked(cycle_initial=|_, _, _| false, heap_size=ruff_memory_usage::heap_size)]
    pub fn is_typed_dict(self, db: &'db dyn Db) -> bool {
        if let Some(known) = self.known(db) {
            return known.is_typed_dict_subclass();
        }

        self.iter_mro(db, None).contains(&ClassBase::TypedDict)
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

        let module = parsed_module(db, self.file(db)).load(db);
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
            CodeGeneratorKind::from_class(db, self.into(), None)?
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
    fn has_dataclass_param(
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
        self.dataclass_transformer_params(db).or_else(|| {
            self.iter_mro(db, specialization).skip(1).find_map(|base| {
                base.into_class().and_then(|class| {
                    class
                        .static_class_literal(db)
                        .and_then(|(lit, _)| lit.dataclass_transformer_params(db))
                })
            })
        })
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
        self.try_metaclass(db)
            .map(|(ty, _)| ty)
            .unwrap_or_else(|_| SubclassOfType::subclass_of_unknown())
    }

    /// Return the metaclass of this class, or an error if the metaclass cannot be inferred.
    #[salsa::tracked(
        cycle_initial=|_, _, _| Err(MetaclassError {
            kind: MetaclassErrorKind::Cycle,
        }),
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(crate) fn try_metaclass(
        self,
        db: &'db dyn Db,
    ) -> Result<(Type<'db>, Option<MetaclassTransformInfo<'db>>), MetaclassError<'db>> {
        tracing::trace!("StaticClassLiteral::try_metaclass: {}", self.name(db));

        // Identify the class's own metaclass (or take the first base class's metaclass).
        let mut base_classes = self.fully_static_explicit_bases(db).peekable();

        if base_classes.peek().is_some() && self.inheritance_cycle(db).is_some() {
            // We emit diagnostics for cyclic class definitions elsewhere.
            // Avoid attempting to infer the metaclass if the class is cyclically defined.
            return Ok((SubclassOfType::subclass_of_unknown(), None));
        }

        if self.try_mro(db, None).is_err_and(StaticMroError::is_cycle) {
            return Ok((SubclassOfType::subclass_of_unknown(), None));
        }

        let module = parsed_module(db, self.file(db)).load(db);

        let explicit_metaclass = self.explicit_metaclass(db, &module);

        // Generic metaclasses parameterized by type variables are not supported.
        // `metaclass=Meta[int]` is fine, but `metaclass=Meta[T]` is not.
        // See: https://typing.python.org/en/latest/spec/generics.html#generic-metaclasses
        if let Some(Type::GenericAlias(alias)) = explicit_metaclass {
            let specialization_has_typevars = alias
                .specialization(db)
                .types(db)
                .iter()
                .any(|ty| ty.has_typevar_or_typevar_instance(db));
            if specialization_has_typevars {
                return Err(MetaclassError {
                    kind: MetaclassErrorKind::GenericMetaclass,
                });
            }
        }

        let (metaclass, class_metaclass_was_from) = if let Some(metaclass) = explicit_metaclass {
            (metaclass, self)
        } else if let Some(base_class) = base_classes.next() {
            // For dynamic classes, we can't get a StaticClassLiteral, so use self for tracking.
            let base_class_literal = base_class
                .static_class_literal(db)
                .map(|(lit, _)| lit)
                .unwrap_or(self);
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
                .to_specialized_instance(db, &[KnownClass::Str.to_instance(db), Type::any()]);

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
            // For dynamic classes, we can't get a StaticClassLiteral, so use self for tracking.
            let base_class_literal = base_class
                .static_class_literal(db)
                .map(|(lit, _)| lit)
                .unwrap_or(self);
            if metaclass.is_subclass_of(db, candidate.metaclass) {
                candidate = MetaclassCandidate {
                    metaclass,
                    explicit_metaclass_of: base_class_literal,
                };
                continue;
            }
            if candidate.metaclass.is_subclass_of(db, metaclass) {
                continue;
            }
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

        let transform_info = candidate
            .metaclass
            .static_class_literal(db)
            .and_then(|(metaclass_literal, specialization)| {
                metaclass_literal.inherited_dataclass_transformer_params(db, specialization)
            })
            .map(|params| MetaclassTransformInfo {
                params,
                from_explicit_metaclass: candidate.explicit_metaclass_of == self,
            });
        Ok((candidate.metaclass.into(), transform_info))
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

    pub(super) fn class_member_inner(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        self.class_member_from_mro(db, name, policy, self.iter_mro(db, specialization))
    }

    pub(crate) fn class_member_from_mro(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
        mro_iter: impl Iterator<Item = ClassBase<'db>>,
    ) -> PlaceAndQualifiers<'db> {
        let result = MroLookup::new(db, mro_iter).class_member(
            name,
            policy,
            self.inherited_generic_context(db),
            self.is_known(db, KnownClass::Object),
        );

        match result {
            ClassMemberResult::Done(result) => result.finalize(db),
            ClassMemberResult::TypedDict => {
                typed_dict_class_member(db, ClassLiteral::Static(self), policy, name)
            }
        }
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
        inherited_generic_context: Option<GenericContext<'db>>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Member<'db> {
        // Check if this class is dataclass-like (either via @dataclass or via dataclass_transform)
        if matches!(
            CodeGeneratorKind::from_class(db, self.into(), specialization),
            Some(CodeGeneratorKind::DataclassLike(_))
        ) {
            if name == "__dataclass_fields__" {
                // Make this class look like a subclass of the `DataClassInstance` protocol
                return Member {
                    inner: Place::declared(KnownClass::Dict.to_specialized_instance(
                        db,
                        &[
                            KnownClass::Str.to_instance(db),
                            KnownClass::Field.to_specialized_instance(db, &[Type::any()]),
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

        if CodeGeneratorKind::NamedTuple.matches(db, self.into(), specialization) {
            if let Some(field) = self
                .own_fields(db, specialization, CodeGeneratorKind::NamedTuple)
                .get(name)
            {
                let property_getter_signature = Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_only(Some(Name::new_static("self")))],
                    ),
                    field.declared_ty,
                );
                let property_getter = Type::single_callable(db, property_getter_signature);
                let property = PropertyInstanceType::new(db, Some(property_getter), None, None);
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

        // For dataclass-like classes, `KW_ONLY` sentinel fields are not real
        // class attributes; they are markers used by the dataclass decorator to
        // indicate that subsequent fields are keyword-only. Treat them as
        // undefined so the MRO falls through to parent classes.
        if member
            .inner
            .place
            .raw_type()
            .is_some_and(|ty| ty.is_instance_of(db, KnownClass::KwOnly))
            && CodeGeneratorKind::from_static_class(db, self, None)
                .is_some_and(|policy| matches!(policy, CodeGeneratorKind::DataclassLike(_)))
        {
            return Member::unbound();
        }

        // For enum classes, `nonmember(value)` creates a non-member attribute.
        // At runtime, the enum metaclass unwraps the value, so accessing the attribute
        // returns the inner value, not the `nonmember` wrapper.
        if let Some(ty) = member.inner.place.raw_type() {
            if let Some(value_ty) = try_unwrap_nonmember_value(db, ty) {
                if is_enum_class_by_inheritance(db, self) {
                    return Member::definitely_declared(value_ty);
                }
            }
        }

        member
    }

    /// Returns the type of a synthesized dataclass member like `__init__` or `__lt__`, or
    /// a synthesized `__new__` method for a `NamedTuple`.
    pub(crate) fn own_synthesized_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        inherited_generic_context: Option<GenericContext<'db>>,
        name: &str,
    ) -> Option<Type<'db>> {
        // Handle `@functools.total_ordering`: synthesize comparison methods
        // for classes that have `@total_ordering` and define at least one
        // ordering method. The decorator requires at least one of __lt__,
        // __le__, __gt__, or __ge__ to be defined (either in this class or
        // inherited from a superclass, excluding `object`).
        //
        // Only synthesize methods that are not already defined in the MRO.
        // Note: We use direct scope lookups here to avoid infinite recursion
        // through `own_class_member` -> `own_synthesized_member`.
        if self.total_ordering(db)
            && matches!(name, "__lt__" | "__le__" | "__gt__" | "__ge__")
            && !self
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
            && let Some(callables) = root_method_ty.try_upcast_to_callable(db)
        {
            let bool_ty = KnownClass::Bool.to_instance(db);
            let synthesized_callables = callables.map(|callable| {
                let signatures = CallableSignature::from_overloads(
                    callable.signatures(db).iter().map(|signature| {
                        // The generated methods return a union of the root method's return type
                        // and `bool`. This is because `@total_ordering` synthesizes methods like:
                        //     def __gt__(self, other): return not (self == other or self < other)
                        // If `__lt__` returns `int`, then `__gt__` could return `int | bool`.
                        let return_ty =
                            UnionType::from_two_elements(db, signature.return_ty, bool_ty);
                        Signature::new_generic(
                            signature.generic_context,
                            signature.parameters().clone(),
                            return_ty,
                        )
                    }),
                );
                CallableType::new(db, signatures, CallableTypeKind::FunctionLike)
            });

            return Some(synthesized_callables.into_type(db));
        }

        let field_policy = CodeGeneratorKind::from_class(db, self.into(), specialization)?;

        let instance_ty =
            Type::instance(db, self.apply_optional_specialization(db, specialization));

        let signature_from_fields = |mut parameters: Vec<_>, return_ty: Type<'db>| {
            for (field_name, field) in self.fields(db, specialization, field_policy) {
                let (init, mut default_ty, kw_only, alias, converter) = match &field.kind {
                    FieldKind::NamedTuple { default_ty } => (true, *default_ty, None, None, None),
                    FieldKind::Dataclass {
                        init,
                        default_ty,
                        kw_only,
                        alias,
                        converter,
                        ..
                    } => (*init, *default_ty, *kw_only, alias.as_ref(), *converter),
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
                        field_ty = dunder_set.bindings(db).map_types(db, |binding| {
                            let mut value_types = UnionBuilder::new(db);
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
                                .try_call_dunder_get(db, None, Type::from(self))
                                .map(|(return_ty, _)| return_ty)
                                .unwrap_or_else(Type::unknown);
                        }
                    }
                }

                if let Some((converter_input_ty, _)) = converter {
                    field_ty = converter_input_ty;
                }

                let is_kw_only =
                    matches!(name, "__replace__" | "_replace") || kw_only.unwrap_or(false);

                // Use the alias name if provided, otherwise use the field name
                let parameter_name =
                    Name::new(alias.map(|alias| &**alias).unwrap_or(&**field_name));

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
                if !self.has_dataclass_param(db, field_policy, DataclassFlags::INIT) {
                    return None;
                }

                let self_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                    // TODO: could be `Self`.
                    .with_annotated_type(instance_ty);
                signature_from_fields(vec![self_parameter], Type::none(db))
            }
            (
                CodeGeneratorKind::NamedTuple,
                "__new__" | "__init__" | "_replace" | "__replace__" | "_fields",
            ) if self.namedtuple_base_has_unknown_fields(db) => {
                // When the namedtuple base has unknown fields, fall back to NamedTupleFallback
                // which has generic signatures that accept any arguments.
                KnownClass::NamedTupleFallback
                    .to_class_literal(db)
                    .as_class_literal()?
                    .as_static()?
                    .own_class_member(db, inherited_generic_context, None, name)
                    .ignore_possibly_undefined()
                    .map(|ty| {
                        ty.apply_type_mapping(
                            db,
                            &TypeMapping::ReplaceSelf {
                                new_upper_bound: instance_ty,
                            },
                            TypeContext::default(),
                        )
                    })
            }
            (
                CodeGeneratorKind::NamedTuple,
                "__new__" | "_replace" | "__replace__" | "_fields" | "__slots__",
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
                    name,
                    instance_ty,
                    fields_iter,
                    specialization.map(|s| s.generic_context(db)),
                )
            }
            (CodeGeneratorKind::DataclassLike(_), "__lt__" | "__le__" | "__gt__" | "__ge__") => {
                if !self.has_dataclass_param(db, field_policy, DataclassFlags::ORDER) {
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
                    KnownClass::Bool.to_instance(db),
                );

                Some(Type::function_like_callable(db, signature))
            }
            (CodeGeneratorKind::DataclassLike(_), "__hash__") => {
                let unsafe_hash =
                    self.has_dataclass_param(db, field_policy, DataclassFlags::UNSAFE_HASH);
                let frozen = self.has_dataclass_param(db, field_policy, DataclassFlags::FROZEN);
                let eq = self.has_dataclass_param(db, field_policy, DataclassFlags::EQ);

                if unsafe_hash || (frozen && eq) {
                    let signature = Signature::new(
                        Parameters::new(
                            db,
                            [Parameter::positional_or_keyword(Name::new_static("self"))
                                .with_annotated_type(instance_ty)],
                        ),
                        KnownClass::Int.to_instance(db),
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
                Some(Type::heterogeneous_tuple(db, match_args))
            }
            (CodeGeneratorKind::DataclassLike(_), "__weakref__")
                if Program::get(db).python_version(db) >= PythonVersion::PY311 =>
            {
                if !self.has_dataclass_param(db, field_policy, DataclassFlags::WEAKREF_SLOT)
                    || !self.has_dataclass_param(db, field_policy, DataclassFlags::SLOTS)
                {
                    return None;
                }

                // This could probably be `weakref | None`, but it does not seem important enough to
                // model it precisely.
                Some(UnionType::from_two_elements(
                    db,
                    Type::any(),
                    Type::none(db),
                ))
            }
            (CodeGeneratorKind::NamedTuple, name) if name != "__init__" => {
                KnownClass::NamedTupleFallback
                    .to_class_literal(db)
                    .as_class_literal()?
                    .as_static()?
                    .own_class_member(db, self.inherited_generic_context(db), None, name)
                    .ignore_possibly_undefined()
                    .map(|ty| {
                        ty.apply_type_mapping(
                            db,
                            &TypeMapping::ReplaceSelf {
                                new_upper_bound: determine_upper_bound(
                                    db,
                                    ClassLiteral::Static(self),
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

                signature_from_fields(vec![self_parameter], instance_ty)
            }
            (CodeGeneratorKind::DataclassLike(_), "__setattr__") => {
                if self.is_frozen_dataclass(db) == Some(true) {
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
                        Type::Never,
                    );

                    return Some(Type::function_like_callable(db, signature));
                }
                None
            }
            (CodeGeneratorKind::DataclassLike(_), "__slots__")
                if Program::get(db).python_version(db) >= PythonVersion::PY310 =>
            {
                self.has_dataclass_param(db, field_policy, DataclassFlags::SLOTS)
                    .then(|| {
                        let fields = self.fields(db, specialization, field_policy);
                        let slots = fields.keys().map(|name| Type::string_literal(db, name));
                        Type::heterogeneous_tuple(db, slots)
                    })
            }
            (CodeGeneratorKind::TypedDict, name) => {
                synthesize_typed_dict_method(db, instance_ty, name, || {
                    TypedDictFields::Static(self.fields(db, specialization, field_policy))
                })
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
                .map_type(|ty| {
                    let new_upper_bound = determine_upper_bound(
                        db,
                        ClassLiteral::Static(self),
                        ClassBase::is_typed_dict
                    );
                    ty.apply_type_mapping(
                        db,
                        &TypeMapping::ReplaceSelf { new_upper_bound },
                        TypeContext::default(),
                    )
                })
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

        self.iter_mro(db, specialization)
            .rev()
            .filter_map(|superclass| {
                let class = superclass.into_class()?;

                if let Some((class_literal, specialization)) = class.static_class_literal(db) {
                    if field_policy.matches(db, class_literal.into(), specialization) {
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
                FieldSource::Static(class, specialization) => Either::Left(
                    class
                        .own_fields(db, specialization, field_policy)
                        .iter()
                        .map(|(name, field)| (name.clone(), field.clone())),
                ),
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
            .collect()
    }

    pub(crate) fn validate_members(self, context: &InferContext<'db, '_>) {
        let db = context.db();
        let Some(field_policy) = CodeGeneratorKind::from_static_class(db, self, None) else {
            return;
        };
        let class_body_scope = self.body_scope(db);
        let table = place_table(db, class_body_scope);
        let use_def = use_def_map(db, class_body_scope);
        for (symbol_id, declarations) in use_def.all_end_of_scope_symbol_declarations() {
            let result = place_from_declarations(db, declarations.clone());
            let attr = result.ignore_conflicting_declarations();
            let symbol = table.symbol(symbol_id);
            let name = symbol.name();

            let Some(Type::FunctionLiteral(literal)) = attr.place.ignore_possibly_undefined()
            else {
                continue;
            };

            match name.as_str() {
                "__setattr__" | "__delattr__" => {
                    if let CodeGeneratorKind::DataclassLike(_) = field_policy
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
                    if let CodeGeneratorKind::DataclassLike(_) = field_policy
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
    /// we return a map `{"x": Field, "y": Field, "z": Field}` where each `Field` contains
    /// the annotated type, default value (if any), and field properties.
    ///
    /// **Important**: The returned `Field` objects represent our full understanding of the fields,
    /// including properties inherited from class-level dataclass parameters (like `kw_only=True`)
    /// and dataclass-transform parameters (like `kw_only_default=True`). They do not represent
    /// only what is explicitly specified in each field definition.
    #[salsa::tracked(
        returns(ref),
        cycle_initial=|_, _, _, _, _| FxIndexMap::default(),
        heap_size=get_size2::GetSize::get_heap_size
    )]
    pub(crate) fn own_fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> FxIndexMap<Name, Field<'db>> {
        let mut attributes = FxIndexMap::default();

        let class_body_scope = self.body_scope(db);
        let table = place_table(db, class_body_scope);

        let use_def = use_def_map(db, class_body_scope);

        let typed_dict_params = self.typed_dict_params(db);
        let dataclass_kw_only_default = matches!(field_policy, CodeGeneratorKind::DataclassLike(_))
            .then(|| self.has_dataclass_param(db, field_policy, DataclassFlags::KW_ONLY));
        let mut kw_only_sentinel_field_seen = false;

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
                let mut converter = None;
                if let Some(Type::KnownInstance(KnownInstanceType::Field(field))) = default_ty {
                    default_ty = field.default_type(db);
                    init = field.init(db);
                    kw_only = field.kw_only(db);
                    alias = field.alias(db);
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

        match MroLookup::new(db, self.iter_mro(db, specialization)).instance_member(name) {
            InstanceMemberResult::Done(result) => result,
            InstanceMemberResult::TypedDict => KnownClass::TypedDictFallback
                .to_instance(db)
                .instance_member(db, name)
                .map_type(|ty| {
                    ty.apply_type_mapping(
                        db,
                        &TypeMapping::ReplaceSelf {
                            new_upper_bound: Type::instance(db, self.unknown_specialization(db)),
                        },
                        TypeContext::default(),
                    )
                }),
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
        cycle_initial=|_, id, _, _, _| Member {
            inner: Place::bound(Type::divergent(id)).into(),
        },
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(super) fn implicit_attribute_inner(
        db: &'db dyn Db,
        class_body_scope: ScopeId<'db>,
        name: String,
        target_method_decorator: MethodDecorator,
    ) -> Member<'db> {
        // If we do not see any declarations of an attribute, neither in the class body nor in
        // any method, we build a union of the raw types inferred from all bindings of that
        // attribute, then apply public-type promotion to the final union.
        let mut union_of_inferred_types = UnionBuilder::new(db);
        let mut qualifiers = TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE;

        let mut is_attribute_bound = false;

        let file = class_body_scope.file(db);
        let module = parsed_module(db, file).load(db);
        let index = semantic_index(db, file);
        let class_map = use_def_map(db, class_body_scope);
        let class_table = place_table(db, class_body_scope);
        let is_valid_scope = |method_scope: &Scope| {
            let Some(method_def) = method_scope.node().as_function() else {
                return true;
            };

            // Check the decorators directly on the AST node to determine if this method
            // is a classmethod or staticmethod. This is more reliable than checking the
            // final evaluated type, which may be wrapped by other decorators like @cache.
            let function_node = method_def.node(&module);
            let definition = index.expect_single_definition(method_def);

            let mut is_classmethod = false;
            let mut is_staticmethod = false;

            for decorator in &function_node.decorator_list {
                let decorator_ty =
                    definition_expression_type(db, definition, &decorator.expression);
                if let Type::ClassLiteral(class) = decorator_ty {
                    match class.known(db) {
                        Some(KnownClass::Classmethod) => is_classmethod = true,
                        Some(KnownClass::Staticmethod) => is_staticmethod = true,
                        _ => {}
                    }
                }
            }

            // Also check for implicit classmethods/staticmethods based on method name
            let method_name = function_node.name.as_str();
            if is_implicit_classmethod(method_name) {
                is_classmethod = true;
            }
            if is_implicit_staticmethod(method_name) {
                is_staticmethod = true;
            }

            match target_method_decorator {
                MethodDecorator::None => !is_classmethod && !is_staticmethod,
                MethodDecorator::ClassMethod => is_classmethod,
                MethodDecorator::StaticMethod => is_staticmethod,
            }
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
                        .reachable_symbol_bindings(method_place)
                        .find_map(|bind| {
                            (bind.binding.is_defined_and(|def| def == method))
                                .then(|| binding_reachability(db, class_map, &bind))
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

                let inferred_ty = match binding.kind(db) {
                    DefinitionKind::AnnotatedAssignment(_) => {
                        // Annotated assignments were handled above. This branch is not
                        // unreachable (because of the `continue` above), but there is
                        // nothing to do here.
                        None
                    }
                    DefinitionKind::Assignment(assign) => match assign.target_kind() {
                        TargetKind::Sequence(_, unpack) => {
                            // We found an unpacking assignment like:
                            //
                            //     .., self.name, .. = <value>
                            //     (.., self.name, ..) = <value>
                            //     [.., self.name, ..] = <value>

                            let unpacked = infer_unpack_types(db, unpack);
                            Some(unpacked.expression_type(assign.target(&module)))
                        }
                        TargetKind::Single => {
                            // We found an un-annotated attribute assignment of the form:
                            //
                            //     self.name = <value>

                            Some(infer_expression_type(
                                db,
                                index.expression(assign.value(&module)),
                                TypeContext::default(),
                            ))
                        }
                    },
                    DefinitionKind::For(for_stmt) => match for_stmt.target_kind() {
                        TargetKind::Sequence(_, unpack) => {
                            // We found an unpacking assignment like:
                            //
                            //     for .., self.name, .. in <iterable>:

                            let unpacked = infer_unpack_types(db, unpack);
                            Some(unpacked.expression_type(for_stmt.target(&module)))
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
                            Some(iterable_ty.iterate(db).homogeneous_element_type(db))
                        }
                    },
                    DefinitionKind::WithItem(with_item) => match with_item.target_kind() {
                        TargetKind::Sequence(_, unpack) => {
                            // We found an unpacking assignment like:
                            //
                            //     with <context_manager> as .., self.name, ..:

                            let unpacked = infer_unpack_types(db, unpack);
                            Some(unpacked.expression_type(with_item.target(&module)))
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
                            Some(if with_item.is_async() {
                                context_ty.aenter(db)
                            } else {
                                context_ty.enter(db)
                            })
                        }
                    },
                    DefinitionKind::Comprehension(comprehension) => {
                        match comprehension.target_kind() {
                            TargetKind::Sequence(_, unpack) => {
                                // We found an unpacking assignment like:
                                //
                                //     [... for .., self.name, .. in <iterable>]

                                let unpacked = infer_unpack_types(db, unpack);
                                Some(unpacked.expression_type(comprehension.target(&module)))
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
                                Some(iterable_ty.iterate(db).homogeneous_element_type(db))
                            }
                        }
                    }
                    DefinitionKind::AugmentedAssignment(_) => {
                        // TODO:
                        None
                    }
                    DefinitionKind::NamedExpression(_) => {
                        // A named expression whose target is an attribute is syntactically prohibited
                        None
                    }
                    _ => None,
                };

                if let Some(inferred_ty) = inferred_ty {
                    union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                }
            }
        }

        Member {
            inner: if is_attribute_bound {
                Place::bound(
                    union_of_inferred_types
                        .build()
                        .promote(db)
                        .promote_singletons(db),
                )
                .with_qualifiers(qualifiers)
            } else {
                Place::Undefined.with_qualifiers(qualifiers)
            },
        }
    }

    /// A helper function for `instance_member` that looks up the `name` attribute only on
    /// this class, not on its superclasses.
    pub(super) fn own_instance_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        // TODO: There are many things that are not yet implemented here:
        // - `typing.Final`
        // - Proper diagnostics

        // NamedTuple fields are modeled via synthesized descriptors on the class. Treating them
        // as instance attributes here causes inherited fields to leak through after a subclass
        // shadows the name with a normal class attribute.
        if CodeGeneratorKind::NamedTuple.matches(db, self.into(), None)
            && self
                .own_fields(db, None, CodeGeneratorKind::NamedTuple)
                .contains_key(name)
        {
            return Member::unbound();
        }

        let body_scope = self.body_scope(db);
        let table = place_table(db, body_scope);

        if let Some(symbol_id) = table.symbol_id(name) {
            let use_def = use_def_map(db, body_scope);

            let declarations = use_def.end_of_scope_symbol_declarations(symbol_id);
            let declared_and_qualifiers =
                place_from_declarations(db, declarations).ignore_conflicting_declarations();

            match declared_and_qualifiers {
                PlaceAndQualifiers {
                    place:
                        mut declared @ Place::Defined(DefinedPlace {
                            ty: declared_ty,
                            definedness: declaredness,
                            ..
                        }),
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

                    // `KW_ONLY` sentinels are markers, not real instance attributes.
                    if declared_ty.is_instance_of(db, KnownClass::KwOnly)
                        && CodeGeneratorKind::from_static_class(db, self, None).is_some_and(
                            |policy| matches!(policy, CodeGeneratorKind::DataclassLike(_)),
                        )
                    {
                        return Member::unbound();
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
                                    inner: Place::Defined(DefinedPlace {
                                        ty: UnionType::from_two_elements(
                                            db,
                                            declared_ty,
                                            implicit_ty,
                                        ),
                                        origin: TypeOrigin::Declared,
                                        definedness: declaredness,
                                        public_type_policy: PublicTypePolicy::Raw,
                                    })
                                    .with_qualifiers(qualifiers),
                                }
                            }
                        } else if self.is_own_dataclass_instance_field(db, name)
                            && declared_ty
                                .class_member(db, "__get__".into())
                                .place
                                .is_undefined()
                        {
                            // For dataclass-like classes, declared fields are assigned
                            // by the synthesized `__init__`, so they are instance
                            // attributes even without an explicit `self.x = ...`
                            // assignment in a method body.
                            //
                            // However, if the declared type is a descriptor (has
                            // `__get__`), we return unbound so that the descriptor
                            // protocol in `member_lookup_with_policy` can resolve
                            // the attribute type through `__get__`.
                            Member {
                                inner: declared.with_qualifiers(qualifiers),
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
                                    inner: Place::Defined(DefinedPlace {
                                        ty: UnionType::from_two_elements(
                                            db,
                                            declared_ty,
                                            implicit_ty,
                                        ),
                                        origin: TypeOrigin::Declared,
                                        definedness: declaredness,
                                        public_type_policy: PublicTypePolicy::Raw,
                                    })
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

    /// Returns `true` if `name` is a non-init-only field directly declared on this
    /// dataclass (i.e., a field that corresponds to an instance attribute).
    ///
    /// This is used to decide whether a bare class-body annotation like `x: int`
    /// should be treated as defining an instance attribute: dataclass fields are
    /// implicitly assigned in `__init__`, so they behave as instance attributes
    /// even though no explicit binding exists in the class body.
    fn is_own_dataclass_instance_field(self, db: &'db dyn Db, name: &str) -> bool {
        let Some(field_policy) = CodeGeneratorKind::from_static_class(db, self, None) else {
            return false;
        };
        if !matches!(field_policy, CodeGeneratorKind::DataclassLike(_)) {
            return false;
        }

        let fields = self.own_fields(db, None, field_policy);
        let Some(field) = fields.get(name) else {
            return false;
        };
        matches!(
            field.kind,
            FieldKind::Dataclass {
                init_only: false,
                ..
            }
        )
    }

    /// Returns the converter's input type (i.e., the type of its first positional parameter) for a
    /// dataclass field, if the field has a converter function specified.
    pub(super) fn converter_input_type_for_field(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> Option<Type<'db>> {
        let field_policy = CodeGeneratorKind::from_static_class(db, self, None)?;
        if !matches!(field_policy, CodeGeneratorKind::DataclassLike(_)) {
            return None;
        }
        let fields = self.fields(db, None, field_policy);
        let field = fields.get(name)?;
        if let FieldKind::Dataclass { converter, .. } = field.kind {
            converter.map(|(input_ty, _)| input_ty)
        } else {
            None
        }
    }

    pub(super) fn to_non_generic_instance(self, db: &'db dyn Db) -> Type<'db> {
        Type::instance(db, ClassType::NonGeneric(self.into()))
    }

    /// Return this class' involvement in an inheritance cycle, if any.
    ///
    /// A class definition like this will fail at runtime,
    /// but we must be resilient to it or we could panic.
    #[salsa::tracked(cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn inheritance_cycle(self, db: &'db dyn Db) -> Option<InheritanceCycle> {
        /// Return `true` if the class is cyclically defined.
        ///
        /// Also, populates `visited_classes` with all base classes of `self`.
        fn is_cyclically_defined_recursive<'db>(
            db: &'db dyn Db,
            class: StaticClassLiteral<'db>,
            classes_on_stack: &mut FxIndexSet<StaticClassLiteral<'db>>,
            visited_classes: &mut FxIndexSet<StaticClassLiteral<'db>>,
        ) -> bool {
            let mut result = false;
            for explicit_base in class.explicit_bases(db) {
                let explicit_base_class_literal = match explicit_base {
                    Type::ClassLiteral(class_literal) => class_literal.as_static(),
                    Type::GenericAlias(generic_alias) => Some(generic_alias.origin(db)),
                    _ => continue,
                };
                let Some(explicit_base_class_literal) = explicit_base_class_literal else {
                    continue;
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

        let visited_classes = &mut FxIndexSet::default();
        if !is_cyclically_defined_recursive(db, self, &mut FxIndexSet::default(), visited_classes) {
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
}

/// A single semantic class-base entry after expanding starred tuple bases and synthetic bases.
#[derive(Clone, Copy)]
pub(crate) enum ExpandedClassBaseEntry<'a, 'db> {
    /// A base that comes from a concrete expression in the class header.
    SourceBacked { node: &'a ast::Expr, ty: Type<'db> },
    /// A base introduced by semantic expansion with no corresponding source expression.
    Synthetic(Type<'db>),
}

impl<'a, 'db> ExpandedClassBaseEntry<'a, 'db> {
    /// Returns the source expression for this base entry, if it has one.
    pub(crate) const fn source_node(self) -> Option<&'a ast::Expr> {
        match self {
            Self::SourceBacked { node, .. } => Some(node),
            Self::Synthetic(_) => None,
        }
    }

    /// Returns the semantic type of this base entry.
    pub(crate) const fn ty(self) -> Type<'db> {
        match self {
            Self::SourceBacked { ty, .. } | Self::Synthetic(ty) => ty,
        }
    }
}

/// Expands a class's bases into the semantic entries used by [`StaticClassLiteral::explicit_bases`].
///
/// Entries are source-backed when they originate from a concrete base expression in the class
/// header, and synthetic when semantic expansion adds a base with no corresponding source span.
pub(crate) fn expanded_class_base_entries<'a, 'db>(
    db: &'db dyn Db,
    known_class: Option<KnownClass>,
    class_stmt: &'a ast::StmtClassDef,
    class_definition: Definition<'db>,
) -> Vec<ExpandedClassBaseEntry<'a, 'db>> {
    match known_class {
        Some(KnownClass::VersionInfo) => {
            let tuple_type = TupleType::new(db, &TupleSpec::version_info_spec(db))
                .expect("sys.version_info tuple spec should always be a valid tuple");

            vec![
                ExpandedClassBaseEntry::SourceBacked {
                    node: &class_stmt.bases()[0],
                    ty: definition_expression_type(db, class_definition, &class_stmt.bases()[0]),
                },
                ExpandedClassBaseEntry::Synthetic(Type::from(tuple_type.to_class_type(db))),
            ]
        }
        // Special-case `NotImplementedType`: typeshed says that it inherits from `Any`,
        // but this causes more problems than it fixes.
        Some(KnownClass::NotImplementedType) => vec![],
        _ => {
            let mut expanded_bases = Vec::with_capacity(class_stmt.bases().len());

            for base_node in class_stmt.bases() {
                if let Some(tuple) =
                    expanded_fixed_length_starred_class_base_tuple(db, class_definition, base_node)
                {
                    if let ast::Expr::Starred(starred) = base_node
                        && let Some(tuple_literal) = starred.value.as_tuple_expr()
                        && tuple_literal.len() == tuple.len()
                        && tuple_literal
                            .iter()
                            .all(|element| !element.is_starred_expr())
                    {
                        expanded_bases.extend(
                            tuple_literal
                                .iter()
                                .zip(tuple.owned_elements().into_vec())
                                .map(|(node, ty)| ExpandedClassBaseEntry::SourceBacked {
                                    node,
                                    ty,
                                }),
                        );
                        continue;
                    }

                    expanded_bases.extend(tuple.owned_elements().into_vec().into_iter().map(
                        |ty| ExpandedClassBaseEntry::SourceBacked {
                            node: base_node,
                            ty,
                        },
                    ));
                    continue;
                }

                let ty = if matches!(base_node, ast::Expr::Starred(_)) {
                    Type::unknown()
                } else {
                    definition_expression_type(db, class_definition, base_node)
                };
                expanded_bases.push(ExpandedClassBaseEntry::SourceBacked {
                    node: base_node,
                    ty,
                });
            }

            expanded_bases
        }
    }
}

/// If `base_node` is a starred class base whose value is inferred as a fixed-length tuple,
/// returns the unpacked tuple in source order.
fn expanded_fixed_length_starred_class_base_tuple<'db>(
    db: &'db dyn Db,
    class_definition: Definition<'db>,
    base_node: &ast::Expr,
) -> Option<FixedLengthTuple<Type<'db>>> {
    let ast::Expr::Starred(starred) = base_node else {
        return None;
    };

    let starred_ty = definition_expression_type(db, class_definition, &starred.value);
    let tuple_spec = starred_ty.tuple_instance_spec(db)?;
    let Tuple::Fixed(tuple) = tuple_spec.into_owned() else {
        return None;
    };
    Some(tuple)
}

#[salsa::tracked]
impl<'db> VarianceInferable<'db> for StaticClassLiteral<'db> {
    #[salsa::tracked(cycle_initial=|_, _, _, _| TypeVarVariance::Bivariant, heap_size=ruff_memory_usage::heap_size)]
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
            let is_namedtuple = CodeGeneratorKind::NamedTuple.matches(db, self.into(), None);
            // Python 3.13 introduced a synthesized `__replace__` method on dataclasses which uses
            // their field types in contravariant position, thus meaning a frozen dataclass must
            // still be invariant in its field types. Other synthesized methods on dataclasses are
            // not considered here, since they don't use field types in their signatures. TODO:
            // ideally we'd have a single source of truth for information about synthesized
            // methods, so we just look them up normally and don't hardcode this knowledge here.
            let is_frozen_dataclass_prior_to_313 = Program::get(db).python_version(db)
                <= PythonVersion::PY312
                && CodeGeneratorKind::from_static_class(db, self, None)
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

fn explicit_bases_cycle_initial<'db>(
    db: &'db dyn Db,
    id: salsa::Id,
    literal: StaticClassLiteral<'db>,
) -> Box<[Type<'db>]> {
    let module = parsed_module(db, literal.file(db)).load(db);
    let class_stmt = literal.node(db, &module);
    // Try to produce a list of `Divergent` types of the right length. However, if one or more of
    // the bases is a starred expression, we don't know how many entries that will eventually
    // expand to.
    vec![Type::divergent(id); class_stmt.bases().len()].into_boxed_slice()
}

fn explicit_bases_cycle_fn<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &[Type<'db>],
    current: Box<[Type<'db>]>,
    _literal: StaticClassLiteral<'db>,
) -> Box<[Type<'db>]> {
    if previous.len() == current.len() {
        // As long as the length of bases hasn't changed, use the same "monotonic widening"
        // strategy that we use with most types, to avoid oscillations.
        current
            .iter()
            .zip(previous.iter())
            .map(|(curr, prev)| curr.cycle_normalized(db, *prev, cycle))
            .collect()
    } else {
        // The length of bases has changed, presumably because we expanded a starred expression. We
        // don't do "monotonic widening" here, because we don't want to make assumptions about
        // which previous entries correspond to which current ones. An oscillation here would be
        // unfortunate, but maybe only pathological programs can trigger such a thing.
        current
    }
}

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
