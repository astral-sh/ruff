//! Contains representations of function literals. There are several complicating factors:
//!
//! - Functions can be generic, and can have specializations applied to them. These are not the
//!   same thing! For instance, a method of a generic class might not itself be generic, but it can
//!   still have the class's specialization applied to it.
//!
//! - Functions can be overloaded, and each overload can be independently generic or not, with
//!   different sets of typevars for different generic overloads. In some cases we need to consider
//!   each overload separately; in others we need to consider all of the overloads (and any
//!   implementation) as a single collective entity.
//!
//! - Certain “known” functions need special treatment — for instance, inferring a special return
//!   type, or raising custom diagnostics.
//!
//! - TODO: Some functions don't correspond to a function definition in the AST, and are instead
//!   synthesized as we mimic the behavior of the Python interpreter. Even though they are
//!   synthesized, and are “implemented” as Rust code, they are still functions from the POV of the
//!   rest of the type system.
//!
//! Given these constraints, we have the following representation: a function is a list of one or
//! more overloads, with zero or more specializations (more specifically, “type mappings”) applied
//! to it. [`FunctionType`] is the outermost type, which is what [`Type::FunctionLiteral`] wraps.
//! It contains the list of type mappings to apply. It wraps a [`FunctionLiteral`], which collects
//! together all of the overloads (and implementation) of an overloaded function. An
//! [`OverloadLiteral`] represents an individual function definition in the AST — that is, each
//! overload (and implementation) of an overloaded function, or the single definition of a
//! non-overloaded function.
//!
//! Technically, each `FunctionLiteral` wraps a particular overload and all _previous_ overloads.
//! So it's only true that it wraps _all_ overloads if you are looking at the last definition. For
//! instance, in
//!
//! ```py
//! @overload
//! def f(x: int) -> None: ...
//! # <-- 1
//!
//! @overload
//! def f(x: str) -> None: ...
//! # <-- 2
//!
//! def f(x): pass
//! # <-- 3
//! ```
//!
//! resolving `f` at each of the three numbered positions will give you a `FunctionType`, which
//! wraps a `FunctionLiteral`, which contain `OverloadLiteral`s only for the definitions that
//! appear before that position. We rely on the fact that later definitions shadow earlier ones, so
//! the public type of `f` is resolved at position 3, correctly giving you all of the overloads
//! (and the implementation).

use std::str::FromStr;

use bitflags::bitflags;
use ruff_db::diagnostic::{Annotation, DiagnosticId, Severity, Span};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::{self as ast, ParameterWithDefault};
use ruff_text_size::Ranged;

use crate::place::{Definedness, Place, place_from_bindings};
use crate::semantic_index::ast_ids::HasScopedUseId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::scope::ScopeId;
use crate::semantic_index::{FileScopeId, SemanticIndex, semantic_index};
use crate::types::call::{Binding, CallArguments};
use crate::types::constraints::ConstraintSet;
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, REDUNDANT_CAST, STATIC_ASSERT_ERROR, TYPE_ASSERTION_FAILURE,
    report_bad_argument_to_get_protocol_members, report_bad_argument_to_protocol_interface,
    report_runtime_check_against_non_runtime_checkable_protocol,
};
use crate::types::display::DisplaySettings;
use crate::types::generics::{GenericContext, InferableTypeVars, typing_self};
use crate::types::infer::nearest_enclosing_class;
use crate::types::list_members::all_members;
use crate::types::narrow::ClassInfoConstraintFunction;
use crate::types::signatures::{CallableSignature, Signature};
use crate::types::visitor::any_over_type;
use crate::types::{
    ApplyTypeMappingVisitor, BoundMethodType, BoundTypeVarInstance, CallableType, CallableTypeKind,
    ClassBase, ClassLiteral, ClassType, DeprecatedInstance, DynamicType, FindLegacyTypeVarsVisitor,
    HasRelationToVisitor, IsDisjointVisitor, IsEquivalentVisitor, KnownClass, KnownInstanceType,
    NormalizedVisitor, SpecialFormType, SubclassOfInner, SubclassOfType, Truthiness, Type,
    TypeContext, TypeMapping, TypeRelation, TypeVarBoundOrConstraints, UnionBuilder, binding_type,
    definition_expression_type, infer_definition_types, walk_signature,
};
use crate::{Db, FxOrderSet};
use ty_module_resolver::{KnownModule, ModuleName, file_to_module, resolve_module};

/// A collection of useful spans for annotating functions.
///
/// This can be retrieved via `FunctionType::spans` or
/// `Type::function_spans`.
pub(crate) struct FunctionSpans {
    /// The span of the entire function "signature." This includes
    /// the name, parameter list and return type (if present).
    pub(crate) signature: Span,
    /// The span of the function name. i.e., `foo` in `def foo(): ...`.
    pub(crate) name: Span,
    /// The span of the parameter list, including the opening and
    /// closing parentheses.
    pub(crate) parameters: Span,
    /// The span of the annotated return type, if present.
    pub(crate) return_type: Option<Span>,
}

bitflags! {
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Hash)]
    pub struct FunctionDecorators: u8 {
        /// `@classmethod`
        const CLASSMETHOD = 1 << 0;
        /// `@typing.no_type_check`
        const NO_TYPE_CHECK = 1 << 1;
        /// `@typing.overload`
        const OVERLOAD = 1 << 2;
        /// `@abc.abstractmethod`
        const ABSTRACT_METHOD = 1 << 3;
        /// `@typing.final`
        const FINAL = 1 << 4;
        /// `@staticmethod`
        const STATICMETHOD = 1 << 5;
        /// `@typing.override`
        const OVERRIDE = 1 << 6;
        /// `@typing.type_check_only`
        const TYPE_CHECK_ONLY = 1 << 7;
    }
}

impl get_size2::GetSize for FunctionDecorators {}

impl FunctionDecorators {
    pub(super) fn from_decorator_type(db: &dyn Db, decorator_type: Type) -> Self {
        match decorator_type {
            Type::FunctionLiteral(function) => match function.known(db) {
                Some(KnownFunction::NoTypeCheck) => FunctionDecorators::NO_TYPE_CHECK,
                Some(KnownFunction::Overload) => FunctionDecorators::OVERLOAD,
                Some(KnownFunction::AbstractMethod) => FunctionDecorators::ABSTRACT_METHOD,
                Some(KnownFunction::Final) => FunctionDecorators::FINAL,
                Some(KnownFunction::Override) => FunctionDecorators::OVERRIDE,
                Some(KnownFunction::TypeCheckOnly) => FunctionDecorators::TYPE_CHECK_ONLY,
                _ => FunctionDecorators::empty(),
            },
            Type::ClassLiteral(class) => match class.known(db) {
                Some(KnownClass::Classmethod) => FunctionDecorators::CLASSMETHOD,
                Some(KnownClass::Staticmethod) => FunctionDecorators::STATICMETHOD,
                _ => FunctionDecorators::empty(),
            },
            _ => FunctionDecorators::empty(),
        }
    }
}

bitflags! {
    /// Used for the return type of `dataclass_transform(…)` calls. Keeps track of the
    /// arguments that were passed in. For the precise meaning of the fields, see [1].
    ///
    /// [1]: https://docs.python.org/3/library/typing.html#typing.dataclass_transform
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
    pub struct DataclassTransformerFlags: u8 {
        const EQ_DEFAULT = 1 << 0;
        const ORDER_DEFAULT = 1 << 1;
        const KW_ONLY_DEFAULT = 1 << 2;
        const FROZEN_DEFAULT = 1 << 3;
    }
}

impl get_size2::GetSize for DataclassTransformerFlags {}

impl Default for DataclassTransformerFlags {
    fn default() -> Self {
        Self::EQ_DEFAULT
    }
}

/// Metadata for a dataclass-transformer. Stored inside a `Type::DataclassTransformer(…)`
/// instance that we use as the return type for `dataclass_transform(…)` calls.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct DataclassTransformerParams<'db> {
    pub flags: DataclassTransformerFlags,

    #[returns(deref)]
    pub field_specifiers: Box<[Type<'db>]>,
}

impl get_size2::GetSize for DataclassTransformerParams<'_> {}

/// Whether a function should implicitly be treated as a staticmethod based on its name.
pub(crate) fn is_implicit_staticmethod(function_name: &str) -> bool {
    matches!(function_name, "__new__")
}

/// Whether a function should implicitly be treated as a classmethod based on its name.
pub(crate) fn is_implicit_classmethod(function_name: &str) -> bool {
    matches!(function_name, "__init_subclass__" | "__class_getitem__")
}

/// Representation of a function definition in the AST: either a non-generic function, or a generic
/// function that has not been specialized.
///
/// If a function has multiple overloads, each overload is represented by a separate function
/// definition in the AST, and is therefore a separate `OverloadLiteral` instance.
///
/// # Ordering
/// Ordering is based on the function's id assigned by salsa and not on the function literal's
/// values. The id may change between runs, or when the function literal was garbage collected and
/// recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct OverloadLiteral<'db> {
    /// Name of the function at definition.
    #[returns(ref)]
    pub name: ast::name::Name,

    /// Is this a function that we special-case somehow? If so, which one?
    pub(crate) known: Option<KnownFunction>,

    /// The scope that's created by the function, in which the function body is evaluated.
    pub(crate) body_scope: ScopeId<'db>,

    /// A set of special decorators that were applied to this function
    pub(crate) decorators: FunctionDecorators,

    /// If `Some` then contains the `@warnings.deprecated`
    pub(crate) deprecated: Option<DeprecatedInstance<'db>>,

    /// The arguments to `dataclass_transformer`, if this function was annotated
    /// with `@dataclass_transformer(...)`.
    pub(crate) dataclass_transformer_params: Option<DataclassTransformerParams<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for OverloadLiteral<'_> {}

#[salsa::tracked]
impl<'db> OverloadLiteral<'db> {
    fn with_dataclass_transformer_params(
        self,
        db: &'db dyn Db,
        params: DataclassTransformerParams<'db>,
    ) -> Self {
        Self::new(
            db,
            self.name(db).clone(),
            self.known(db),
            self.body_scope(db),
            self.decorators(db),
            self.deprecated(db),
            Some(params),
        )
    }

    fn file(self, db: &'db dyn Db) -> File {
        // NOTE: Do not use `self.definition(db).file(db)` here, as that could create a
        // cross-module dependency on the full AST.
        self.body_scope(db).file(db)
    }

    pub(crate) fn has_known_decorator(self, db: &dyn Db, decorator: FunctionDecorators) -> bool {
        self.decorators(db).contains(decorator)
    }

    pub(crate) fn is_overload(self, db: &dyn Db) -> bool {
        self.has_known_decorator(db, FunctionDecorators::OVERLOAD)
    }

    /// Returns true if this overload is decorated with `@staticmethod`, or if it is implicitly a
    /// staticmethod.
    pub(crate) fn is_staticmethod(self, db: &dyn Db) -> bool {
        self.has_known_decorator(db, FunctionDecorators::STATICMETHOD)
            || is_implicit_staticmethod(self.name(db))
    }

    /// Returns true if this overload is decorated with `@classmethod`, or if it is implicitly a
    /// classmethod.
    pub(crate) fn is_classmethod(self, db: &dyn Db) -> bool {
        self.has_known_decorator(db, FunctionDecorators::CLASSMETHOD)
            || is_implicit_classmethod(self.name(db))
    }

    pub(crate) fn node<'ast>(
        self,
        db: &dyn Db,
        file: File,
        module: &'ast ParsedModuleRef,
    ) -> &'ast ast::StmtFunctionDef {
        debug_assert_eq!(
            file,
            self.file(db),
            "OverloadLiteral::node() must be called with the same file as the one where \
            the function is defined."
        );

        self.body_scope(db).node(db).expect_function().node(module)
    }

    /// Iterate through the decorators on this function, returning the span of the first one
    /// that matches the given predicate.
    pub(super) fn find_decorator_span(
        self,
        db: &'db dyn Db,
        predicate: impl Fn(Type<'db>) -> bool,
    ) -> Option<Span> {
        let definition = self.definition(db);
        let file = definition.file(db);
        self.node(db, file, &parsed_module(db, file).load(db))
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

    /// Iterate through the decorators on this function, returning the span of the first one
    /// that matches the given [`KnownFunction`].
    pub(super) fn find_known_decorator_span(
        self,
        db: &'db dyn Db,
        needle: KnownFunction,
    ) -> Option<Span> {
        self.find_decorator_span(db, |ty| {
            ty.as_function_literal()
                .is_some_and(|f| f.is_known(db, needle))
        })
    }

    /// Returns the [`FileRange`] of the function's name.
    pub(crate) fn focus_range(self, db: &dyn Db, module: &ParsedModuleRef) -> FileRange {
        FileRange::new(
            self.file(db),
            self.body_scope(db)
                .node(db)
                .expect_function()
                .node(module)
                .name
                .range,
        )
    }

    /// Returns the [`Definition`] of this function.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let body_scope = self.body_scope(db);
        let index = semantic_index(db, body_scope.file(db));
        index.expect_single_definition(body_scope.node(db).expect_function())
    }

    /// Returns the overload immediately before this one in the AST. Returns `None` if there is no
    /// previous overload.
    fn previous_overload(self, db: &'db dyn Db) -> Option<FunctionLiteral<'db>> {
        // The semantic model records a use for each function on the name node. This is used
        // here to get the previous function definition with the same name.
        let scope = self.definition(db).scope(db);
        let module = parsed_module(db, self.file(db)).load(db);
        let use_def = semantic_index(db, scope.file(db)).use_def_map(scope.file_scope_id(db));
        let use_id = self
            .body_scope(db)
            .node(db)
            .expect_function()
            .node(&module)
            .name
            .scoped_use_id(db, scope);

        let Place::Defined(Type::FunctionLiteral(previous_type), _, Definedness::AlwaysDefined, _) =
            place_from_bindings(db, use_def.bindings_at_use(use_id)).place
        else {
            return None;
        };

        let previous_literal = previous_type.literal(db);
        let previous_overload = previous_literal.last_definition(db);
        if !previous_overload.is_overload(db) {
            return None;
        }

        Some(previous_literal)
    }

    /// Typed internally-visible signature for this function.
    ///
    /// This represents the annotations on the function itself, unmodified by decorators and
    /// overloads.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    pub(crate) fn signature(self, db: &'db dyn Db) -> Signature<'db> {
        let mut signature = self.raw_signature(db);

        let scope = self.body_scope(db);
        let module = parsed_module(db, self.file(db)).load(db);
        let function_node = scope.node(db).expect_function().node(&module);
        let index = semantic_index(db, scope.file(db));
        let file_scope_id = scope.file_scope_id(db);
        let is_generator = file_scope_id.is_generator_function(index);

        if function_node.is_async && !is_generator {
            signature = signature.wrap_coroutine_return_type(db);
        }

        signature
    }

    /// Typed internally-visible "raw" signature for this function.
    /// That is, the return types of async functions are not wrapped in `CoroutineType[...]`.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    fn raw_signature(self, db: &'db dyn Db) -> Signature<'db> {
        /// `self` or `cls` can be implicitly positional-only if:
        /// - It is a method AND
        /// - No parameters in the method use PEP-570 syntax AND
        /// - It is not a `@staticmethod` AND
        /// - `self`/`cls` is not explicitly positional-only using the PEP-484 convention AND
        /// - Either the next parameter after `self`/`cls` uses the PEP-484 convention,
        ///   or the enclosing class is a `Protocol` class
        fn has_implicitly_positional_only_first_param<'db>(
            db: &'db dyn Db,
            literal: OverloadLiteral<'db>,
            node: &ast::StmtFunctionDef,
            scope: FileScopeId,
            index: &SemanticIndex,
        ) -> bool {
            let parameters = &node.parameters;

            if !parameters.posonlyargs.is_empty() {
                return false;
            }

            let Some(first_param) = parameters.args.first() else {
                return false;
            };

            if first_param.uses_pep_484_positional_only_convention() {
                return false;
            }

            if literal.is_staticmethod(db) {
                return false;
            }

            let Some(class_definition) = index.class_definition_of_method(scope) else {
                return false;
            };

            // `self` and `cls` are always positional-only if the next parameter uses the
            // PEP-484 convention.
            if parameters
                .args
                .get(1)
                .is_some_and(ParameterWithDefault::uses_pep_484_positional_only_convention)
            {
                return true;
            }

            // If there isn't any parameter other than `self`/`cls`,
            // or there is but it isn't using the PEP-484 convention,
            // then `self`/`cls` are only implicitly positional-only if
            // it is a protocol class.
            let class_type = binding_type(db, class_definition);
            class_type
                .to_class_type(db)
                .is_some_and(|class| class.is_protocol(db))
        }

        let scope = self.body_scope(db);
        let module = parsed_module(db, self.file(db)).load(db);
        let function_stmt_node = scope.node(db).expect_function().node(&module);
        let definition = self.definition(db);
        let index = semantic_index(db, scope.file(db));
        let pep695_ctx = function_stmt_node.type_params.as_ref().map(|type_params| {
            GenericContext::from_type_params(db, index, definition, type_params)
        });
        let file_scope_id = scope.file_scope_id(db);

        let has_implicitly_positional_first_parameter = has_implicitly_positional_only_first_param(
            db,
            self,
            function_stmt_node,
            file_scope_id,
            index,
        );

        let mut raw_signature = Signature::from_function(
            db,
            pep695_ctx,
            definition,
            function_stmt_node,
            has_implicitly_positional_first_parameter,
        );

        let generic_context = raw_signature.generic_context;
        raw_signature.add_implicit_self_annotation(db, || {
            if self.is_staticmethod(db) {
                return None;
            }

            // We have not yet added an implicit annotation to the `self` parameter, so any
            // typevars that currently appear in the method's generic context come from explicit
            // annotations.
            let method_has_explicit_self = generic_context
                .is_some_and(|context| context.variables(db).any(|v| v.typevar(db).is_self(db)));

            let class_scope_id = definition.scope(db);
            let class_scope = index.scope(class_scope_id.file_scope_id(db));
            let class_node = class_scope.node().as_class()?;
            let class_def = index.expect_single_definition(class_node);
            let Type::ClassLiteral(class_literal) = infer_definition_types(db, class_def)
                .declaration_type(class_def)
                .inner_type()
            else {
                return None;
            };
            let class_is_generic = class_literal.generic_context(db).is_some();
            let class_is_fallback = class_literal
                .known(db)
                .is_some_and(KnownClass::is_fallback_class);

            // Normally we implicitly annotate `self` or `cls` with `Self` or `type[Self]`, and
            // create a `Self` typevar that we then have to solve for whenever this method is
            // called. As an optimization, we can skip creating that typevar in certain situations:
            //
            //   - The method cannot use explicit `Self` in any other parameter annotations,
            //     or in its return type. If it does, then we really do need specialization
            //     inference at each call site to see which specific instance type should be
            //     used in those other parameters / return type.
            //
            //   - The class cannot be generic. If it is, then we might need an actual `Self`
            //     typevar to help carry through constraints that relate the instance type to
            //     other typevars in the method signature.
            //
            //   - The class cannot be a "fallback class". A fallback class is used like a mixin,
            //     and so we need specialization inference to determine the "real" class that the
            //     fallback is augmenting. (See KnownClass::is_fallback_class for more details.)
            if method_has_explicit_self || class_is_generic || class_is_fallback {
                let scope_id = definition.scope(db);
                let typevar_binding_context = Some(definition);
                let index = semantic_index(db, scope_id.file(db));
                let class = nearest_enclosing_class(db, index, scope_id).unwrap();

                let typing_self = typing_self(db, scope_id, typevar_binding_context, class).expect(
                    "We should always find the surrounding class \
                     for an implicit self: Self annotation",
                );

                if self.is_classmethod(db) {
                    Some(SubclassOfType::from(
                        db,
                        SubclassOfInner::TypeVar(typing_self),
                    ))
                } else {
                    Some(Type::TypeVar(typing_self))
                }
            } else {
                // If skip creating the typevar, we use "instance of class" or "subclass of
                // class" as the implicit annotation instead.
                if self.is_classmethod(db) {
                    Some(SubclassOfType::from(
                        db,
                        SubclassOfInner::Class(ClassType::NonGeneric(class_literal)),
                    ))
                } else {
                    Some(class_literal.to_non_generic_instance(db))
                }
            }
        });

        raw_signature
    }

    pub(crate) fn parameter_span(
        self,
        db: &'db dyn Db,
        parameter_index: Option<usize>,
    ) -> Option<(Span, Span)> {
        let function_scope = self.body_scope(db);
        let span = Span::from(function_scope.file(db));
        let node = function_scope.node(db);
        let module = parsed_module(db, self.file(db)).load(db);
        let func_def = node.as_function()?.node(&module);
        let range = parameter_index
            .and_then(|parameter_index| {
                func_def
                    .parameters
                    .iter()
                    .nth(parameter_index)
                    .map(|param| param.range())
            })
            .unwrap_or(func_def.parameters.range);
        let name_span = span.clone().with_range(func_def.name.range);
        let parameter_span = span.with_range(range);
        Some((name_span, parameter_span))
    }

    pub(crate) fn spans(self, db: &'db dyn Db) -> Option<FunctionSpans> {
        let function_scope = self.body_scope(db);
        let span = Span::from(function_scope.file(db));
        let node = function_scope.node(db);
        let module = parsed_module(db, self.file(db)).load(db);
        let func_def = node.as_function()?.node(&module);
        let return_type_range = func_def.returns.as_ref().map(|returns| returns.range());
        let mut signature = func_def.name.range.cover(func_def.parameters.range);
        if let Some(return_type_range) = return_type_range {
            signature = signature.cover(return_type_range);
        }
        Some(FunctionSpans {
            signature: span.clone().with_range(signature),
            name: span.clone().with_range(func_def.name.range),
            parameters: span.clone().with_range(func_def.parameters.range),
            return_type: return_type_range.map(|range| span.clone().with_range(range)),
        })
    }
}

/// Representation of a function definition in the AST, along with any previous overloads of the
/// function. Each overload can be separately generic or not, and each generic overload uses
/// distinct typevars.
///
/// # Ordering
/// Ordering is based on the function's id assigned by salsa and not on the function literal's
/// values. The id may change between runs, or when the function literal was garbage collected and
/// recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct FunctionLiteral<'db> {
    pub(crate) last_definition: OverloadLiteral<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for FunctionLiteral<'_> {}

fn overloads_and_implementation_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: FunctionLiteral<'db>,
) -> (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>) {
    (Box::new([]), None)
}

#[salsa::tracked]
impl<'db> FunctionLiteral<'db> {
    fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        // All of the overloads of a function literal should have the same name.
        self.last_definition(db).name(db)
    }

    fn known(self, db: &'db dyn Db) -> Option<KnownFunction> {
        // Whether a function is known is based on its name (and its containing module's name), so
        // all overloads should be known (or not) equivalently.
        self.last_definition(db).known(db)
    }

    fn has_known_decorator(self, db: &dyn Db, decorator: FunctionDecorators) -> bool {
        self.iter_overloads_and_implementation(db)
            .any(|overload| overload.decorators(db).contains(decorator))
    }

    /// If the implementation of this function is deprecated, returns the `@warnings.deprecated`.
    ///
    /// Checking if an overload is deprecated requires deeper call analysis.
    fn implementation_deprecated(self, db: &'db dyn Db) -> Option<DeprecatedInstance<'db>> {
        let (_overloads, implementation) = self.overloads_and_implementation(db);
        implementation.and_then(|overload| overload.deprecated(db))
    }

    fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.last_definition(db).definition(db)
    }

    fn parameter_span(
        self,
        db: &'db dyn Db,
        parameter_index: Option<usize>,
    ) -> Option<(Span, Span)> {
        self.last_definition(db).parameter_span(db, parameter_index)
    }

    fn spans(self, db: &'db dyn Db) -> Option<FunctionSpans> {
        self.last_definition(db).spans(db)
    }

    fn overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> (&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>) {
        #[salsa::tracked(
            returns(ref),
            heap_size=ruff_memory_usage::heap_size,
            cycle_initial=overloads_and_implementation_cycle_initial
        )]
        fn overloads_and_implementation_inner<'db>(
            db: &'db dyn Db,
            function: FunctionLiteral<'db>,
        ) -> (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>) {
            let self_overload = function.last_definition(db);
            let mut current = self_overload;
            let mut overloads = vec![];

            while let Some(previous) = current.previous_overload(db) {
                let overload = previous.last_definition(db);
                overloads.push(overload);
                current = overload;
            }

            // Overloads are inserted in reverse order, from bottom to top.
            overloads.reverse();

            let implementation = if self_overload.is_overload(db) {
                overloads.push(self_overload);
                None
            } else {
                Some(self_overload)
            };

            (overloads.into_boxed_slice(), implementation)
        }

        let (overloads, implementation) = overloads_and_implementation_inner(db, self);
        (overloads.as_ref(), *implementation)
    }

    fn iter_overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = OverloadLiteral<'db>> + 'db {
        let (implementation, overloads) = self.overloads_and_implementation(db);
        overloads.into_iter().chain(implementation.iter().copied())
    }

    /// Typed externally-visible signature for this function.
    ///
    /// This is the signature as seen by external callers, possibly modified by decorators and/or
    /// overloaded.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    fn signature(self, db: &'db dyn Db) -> CallableSignature<'db> {
        // We only include an implementation (i.e. a definition not decorated with `@overload`) if
        // it's the only definition.
        let (overloads, implementation) = self.overloads_and_implementation(db);
        if let Some(implementation) = implementation
            && overloads.is_empty()
        {
            return CallableSignature::single(implementation.signature(db));
        }

        CallableSignature::from_overloads(overloads.iter().map(|overload| overload.signature(db)))
    }

    /// Typed externally-visible signature of the last overload or implementation of this function.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    fn last_definition_signature(self, db: &'db dyn Db) -> Signature<'db> {
        self.last_definition(db).signature(db)
    }

    /// Typed externally-visible "raw" signature of the last overload or implementation of this function.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    fn last_definition_raw_signature(self, db: &'db dyn Db) -> Signature<'db> {
        self.last_definition(db).raw_signature(db)
    }
}

/// Represents a function type, which might be a non-generic function, or a specialization of a
/// generic function.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct FunctionType<'db> {
    pub(crate) literal: FunctionLiteral<'db>,

    /// Contains a potentially modified signature for this function literal, in case certain operations
    /// (like type mappings) have been applied to it.
    ///
    /// See also: [`FunctionLiteral::updated_signature`].
    #[returns(as_ref)]
    updated_signature: Option<CallableSignature<'db>>,

    /// Contains a potentially modified signature for the last overload or the implementation of this
    /// function literal, in case certain operations (like type mappings) have been applied to it.
    ///
    /// See also: [`FunctionLiteral::last_definition_signature`].
    #[returns(as_ref)]
    updated_last_definition_signature: Option<Signature<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for FunctionType<'_> {}

pub(super) fn walk_function_type<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
    visitor: &V,
) {
    if let Some(callable_signature) = function.updated_signature(db) {
        for signature in &callable_signature.overloads {
            walk_signature(db, signature, visitor);
        }
    }
    if let Some(signature) = function.updated_last_definition_signature(db) {
        walk_signature(db, signature, visitor);
    }
}

#[salsa::tracked]
impl<'db> FunctionType<'db> {
    pub(crate) fn with_inherited_generic_context(
        self,
        db: &'db dyn Db,
        inherited_generic_context: GenericContext<'db>,
    ) -> Self {
        let updated_signature = self
            .signature(db)
            .with_inherited_generic_context(db, inherited_generic_context);
        let updated_last_definition_signature = self
            .last_definition_signature(db)
            .clone()
            .with_inherited_generic_context(db, inherited_generic_context);
        Self::new(
            db,
            self.literal(db),
            Some(updated_signature),
            Some(updated_last_definition_signature),
        )
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let updated_signature =
            self.signature(db)
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let updated_last_definition_signature = self
            .last_definition_signature(db)
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        Self::new(
            db,
            self.literal(db),
            Some(updated_signature),
            Some(updated_last_definition_signature),
        )
    }

    pub(crate) fn with_dataclass_transformer_params(
        self,
        db: &'db dyn Db,
        params: DataclassTransformerParams<'db>,
    ) -> Self {
        // A decorator only applies to the specific overload that it is attached to, not to all
        // previous overloads.
        let literal = self.literal(db);
        let last_definition = literal
            .last_definition(db)
            .with_dataclass_transformer_params(db, params);
        let literal = FunctionLiteral::new(db, last_definition);
        Self::new(db, literal, None, None)
    }

    /// Returns the [`File`] in which this function is defined.
    pub(crate) fn file(self, db: &'db dyn Db) -> File {
        self.literal(db).last_definition(db).file(db)
    }

    /// Returns the AST node for this function.
    pub(super) fn node<'ast>(
        self,
        db: &dyn Db,
        file: File,
        module: &'ast ParsedModuleRef,
    ) -> &'ast ast::StmtFunctionDef {
        self.literal(db).last_definition(db).node(db, file, module)
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        self.literal(db).name(db)
    }

    pub(crate) fn known(self, db: &'db dyn Db) -> Option<KnownFunction> {
        self.literal(db).known(db)
    }

    pub(crate) fn is_known(self, db: &'db dyn Db, known_function: KnownFunction) -> bool {
        self.known(db) == Some(known_function)
    }

    /// Returns if any of the overloads of this function have a particular decorator.
    ///
    /// Some decorators are expected to appear on every overload; others are expected to appear
    /// only the implementation or first overload. This method does not check either of those
    /// conditions.
    pub(crate) fn has_known_decorator(self, db: &dyn Db, decorator: FunctionDecorators) -> bool {
        self.literal(db).has_known_decorator(db, decorator)
    }

    /// Returns true if this method is decorated with `@classmethod`, or if it is implicitly a
    /// classmethod.
    pub(crate) fn is_classmethod(self, db: &'db dyn Db) -> bool {
        self.iter_overloads_and_implementation(db)
            .any(|overload| overload.is_classmethod(db))
    }

    /// Returns true if this method is decorated with `@staticmethod`, or if it is implicitly a
    /// static method.
    pub(crate) fn is_staticmethod(self, db: &'db dyn Db) -> bool {
        self.iter_overloads_and_implementation(db)
            .any(|overload| overload.is_staticmethod(db))
    }

    /// If the implementation of this function is deprecated, returns the `@warnings.deprecated`.
    ///
    /// Checking if an overload is deprecated requires deeper call analysis.
    pub(crate) fn implementation_deprecated(
        self,
        db: &'db dyn Db,
    ) -> Option<DeprecatedInstance<'db>> {
        self.literal(db).implementation_deprecated(db)
    }

    /// Returns the [`Definition`] of the implementation or first overload of this function.
    ///
    /// ## Warning
    ///
    /// This uses the semantic index to find the definition of the function. This means that if the
    /// calling query is not in the same file as this function is defined in, then this will create
    /// a cross-module dependency directly on the full AST which will lead to cache
    /// over-invalidation.
    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.literal(db).definition(db)
    }

    /// Returns a tuple of two spans. The first is
    /// the span for the identifier of the function
    /// definition for `self`. The second is
    /// the span for the parameter in the function
    /// definition for `self`.
    ///
    /// If there are no meaningful spans, then this
    /// returns `None`. For example, when this type
    /// isn't callable.
    ///
    /// When `parameter_index` is `None`, then the
    /// second span returned covers the entire parameter
    /// list.
    ///
    /// # Performance
    ///
    /// Note that this may introduce cross-module
    /// dependencies. This can have an impact on
    /// the effectiveness of incremental caching
    /// and should therefore be used judiciously.
    ///
    /// An example of a good use case is to improve
    /// a diagnostic.
    pub(crate) fn parameter_span(
        self,
        db: &'db dyn Db,
        parameter_index: Option<usize>,
    ) -> Option<(Span, Span)> {
        self.literal(db).parameter_span(db, parameter_index)
    }

    /// Returns a collection of useful spans for a
    /// function signature. These are useful for
    /// creating annotations on diagnostics.
    ///
    /// # Performance
    ///
    /// Note that this may introduce cross-module
    /// dependencies. This can have an impact on
    /// the effectiveness of incremental caching
    /// and should therefore be used judiciously.
    ///
    /// An example of a good use case is to improve
    /// a diagnostic.
    pub(crate) fn spans(self, db: &'db dyn Db) -> Option<FunctionSpans> {
        self.literal(db).spans(db)
    }

    /// Returns all of the overload signatures and the implementation definition, if any, of this
    /// function. The overload signatures will be in source order.
    pub(crate) fn overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> (&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>) {
        self.literal(db).overloads_and_implementation(db)
    }

    /// Returns an iterator of all of the definitions of this function, including both overload
    /// signatures and any implementation, all in source order.
    pub(crate) fn iter_overloads_and_implementation(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = OverloadLiteral<'db>> + 'db {
        self.literal(db).iter_overloads_and_implementation(db)
    }

    pub(crate) fn first_overload_or_implementation(self, db: &'db dyn Db) -> OverloadLiteral<'db> {
        self.iter_overloads_and_implementation(db)
            .next()
            .expect("A function must have at least one overload/implementation")
    }

    /// Typed externally-visible signature for this function.
    ///
    /// This is the signature as seen by external callers, possibly modified by decorators and/or
    /// overloaded.
    ///
    /// ## Why is this a salsa query?
    ///
    /// This is a salsa query to short-circuit the invalidation
    /// when the function's AST node changes.
    ///
    /// Were this not a salsa query, then the calling query
    /// would depend on the function's AST and rerun for every change in that file.
    #[salsa::tracked(returns(ref), cycle_initial=signature_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn signature(self, db: &'db dyn Db) -> CallableSignature<'db> {
        self.updated_signature(db)
            .cloned()
            .unwrap_or_else(|| self.literal(db).signature(db))
    }

    /// Typed externally-visible signature of the last overload or implementation of this function.
    ///
    /// ## Why is this a salsa query?
    ///
    /// This is a salsa query to short-circuit the invalidation
    /// when the function's AST node changes.
    ///
    /// Were this not a salsa query, then the calling query
    /// would depend on the function's AST and rerun for every change in that file.
    #[salsa::tracked(
        returns(ref), cycle_initial=last_definition_signature_cycle_initial,
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(crate) fn last_definition_signature(self, db: &'db dyn Db) -> Signature<'db> {
        self.updated_last_definition_signature(db)
            .cloned()
            .unwrap_or_else(|| self.literal(db).last_definition_signature(db))
    }

    /// Typed externally-visible "raw" signature of the last overload or implementation of this function.
    #[salsa::tracked(
        returns(ref), cycle_initial=last_definition_signature_cycle_initial,
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(crate) fn last_definition_raw_signature(self, db: &'db dyn Db) -> Signature<'db> {
        self.literal(db).last_definition_raw_signature(db)
    }

    /// Convert the `FunctionType` into a [`CallableType`].
    pub(crate) fn into_callable_type(self, db: &'db dyn Db) -> CallableType<'db> {
        let kind = if self.is_classmethod(db) {
            CallableTypeKind::ClassMethodLike
        } else if self.is_staticmethod(db) {
            CallableTypeKind::StaticMethodLike
        } else {
            CallableTypeKind::FunctionLike
        };
        CallableType::new(db, self.signature(db), kind)
    }

    /// Convert the `FunctionType` into a [`BoundMethodType`].
    pub(crate) fn into_bound_method_type(
        self,
        db: &'db dyn Db,
        self_instance: Type<'db>,
    ) -> BoundMethodType<'db> {
        BoundMethodType::new(db, self, self_instance)
    }

    pub(crate) fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self.literal(db) != other.literal(db) {
            return ConstraintSet::from(false);
        }

        let self_signature = self.signature(db);
        let other_signature = other.signature(db);
        self_signature.has_relation_to_impl(
            db,
            other_signature,
            inferable,
            relation,
            relation_visitor,
            disjointness_visitor,
        )
    }

    pub(crate) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self.normalized(db) == other.normalized(db) {
            return ConstraintSet::from(true);
        }
        if self.literal(db) != other.literal(db) {
            return ConstraintSet::from(false);
        }
        let self_signature = self.signature(db);
        let other_signature = other.signature(db);
        self_signature.is_equivalent_to_impl(db, other_signature, inferable, visitor)
    }

    pub(crate) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        let signatures = self.signature(db);
        for signature in &signatures.overloads {
            signature.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
    }

    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        self.normalized_impl(db, &NormalizedVisitor::default())
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        let literal = self.literal(db);
        let updated_signature = self
            .updated_signature(db)
            .map(|signature| signature.normalized_impl(db, visitor));
        let updated_last_definition_signature = self
            .updated_last_definition_signature(db)
            .map(|signature| signature.normalized_impl(db, visitor));
        Self::new(
            db,
            literal,
            updated_signature,
            updated_last_definition_signature,
        )
    }

    pub(crate) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let literal = self.literal(db);
        let updated_signature = match self.updated_signature(db) {
            Some(signature) => Some(signature.recursive_type_normalized_impl(db, div, nested)?),
            None => None,
        };
        let updated_last_definition_signature = match self.updated_last_definition_signature(db) {
            Some(signature) => Some(signature.recursive_type_normalized_impl(db, div, nested)?),
            None => None,
        };
        Some(Self::new(
            db,
            literal,
            updated_signature,
            updated_last_definition_signature,
        ))
    }
}

/// Evaluate an `isinstance` call. Return `Truthiness::AlwaysTrue` if we can definitely infer that
/// this will return `True` at runtime, `Truthiness::AlwaysFalse` if we can definitely infer
/// that this will return `False` at runtime, or `Truthiness::Ambiguous` if we should infer `bool`
/// instead.
fn is_instance_truthiness<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    class: ClassLiteral<'db>,
) -> Truthiness {
    let is_instance = |ty: &Type<'_>| {
        if let Type::NominalInstance(instance) = ty
            && instance
                .class(db)
                .iter_mro(db)
                .filter_map(ClassBase::into_class)
                .any(|c| match c {
                    ClassType::Generic(c) => c.origin(db) == class,
                    ClassType::NonGeneric(c) => c == class,
                })
        {
            return true;
        }
        false
    };

    let always_true_if = |test: bool| {
        if test {
            Truthiness::AlwaysTrue
        } else {
            Truthiness::Ambiguous
        }
    };

    match ty {
        Type::Union(..) => {
            // We do not handle unions specifically here, because something like `A | SubclassOfA` would
            // have been simplified to `A` anyway
            Truthiness::Ambiguous
        }
        Type::Intersection(intersection) => always_true_if(
            intersection
                .positive(db)
                .iter()
                .any(|element| is_instance_truthiness(db, *element, class).is_always_true()),
        ),

        Type::NominalInstance(..) => always_true_if(is_instance(&ty)),

        Type::NewTypeInstance(newtype) => {
            always_true_if(is_instance(&newtype.concrete_base_type(db)))
        }

        Type::BooleanLiteral(..)
        | Type::BytesLiteral(..)
        | Type::IntLiteral(..)
        | Type::StringLiteral(..)
        | Type::LiteralString
        | Type::ModuleLiteral(..)
        | Type::EnumLiteral(..)
        | Type::FunctionLiteral(..) => always_true_if(
            ty.literal_fallback_instance(db)
                .as_ref()
                .is_some_and(is_instance),
        ),

        Type::ClassLiteral(..) => always_true_if(is_instance(&KnownClass::Type.to_instance(db))),

        Type::TypeAlias(alias) => is_instance_truthiness(db, alias.value_type(db), class),

        Type::TypeVar(bound_typevar) => match bound_typevar.typevar(db).bound_or_constraints(db) {
            None => is_instance_truthiness(db, Type::object(), class),
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                is_instance_truthiness(db, bound, class)
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => always_true_if(
                constraints
                    .elements(db)
                    .iter()
                    .all(|c| is_instance_truthiness(db, *c, class).is_always_true()),
            ),
        },

        Type::BoundMethod(..)
        | Type::KnownBoundMethod(..)
        | Type::WrapperDescriptor(..)
        | Type::DataclassDecorator(..)
        | Type::DataclassTransformer(..)
        | Type::GenericAlias(..)
        | Type::SubclassOf(..)
        | Type::ProtocolInstance(..)
        | Type::SpecialForm(..)
        | Type::KnownInstance(..)
        | Type::PropertyInstance(..)
        | Type::AlwaysTruthy
        | Type::AlwaysFalsy
        | Type::BoundSuper(..)
        | Type::TypeIs(..)
        | Type::Callable(..)
        | Type::Dynamic(..)
        | Type::Never
        | Type::TypedDict(_) => {
            // We could probably try to infer more precise types in some of these cases, but it's unclear
            // if it's worth the effort.
            Truthiness::Ambiguous
        }
    }
}

fn signature_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _function: FunctionType<'db>,
) -> CallableSignature<'db> {
    CallableSignature::single(Signature::bottom())
}

fn last_definition_signature_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _function: FunctionType<'db>,
) -> Signature<'db> {
    Signature::bottom()
}

/// Non-exhaustive enumeration of known functions (e.g. `builtins.reveal_type`, ...) that might
/// have special behavior.
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    strum_macros::EnumString,
    strum_macros::IntoStaticStr,
    get_size2::GetSize,
)]
#[strum(serialize_all = "snake_case")]
#[cfg_attr(test, derive(strum_macros::EnumIter))]
pub enum KnownFunction {
    /// `builtins.isinstance`
    #[strum(serialize = "isinstance")]
    IsInstance,
    /// `builtins.issubclass`
    #[strum(serialize = "issubclass")]
    IsSubclass,
    /// `builtins.hasattr`
    #[strum(serialize = "hasattr")]
    HasAttr,
    /// `builtins.reveal_type`, `typing.reveal_type` or `typing_extensions.reveal_type`
    RevealType,
    /// `builtins.len`
    Len,
    /// `builtins.repr`
    Repr,
    /// `builtins.__import__`, which returns the top-level module.
    #[strum(serialize = "__import__")]
    DunderImport,
    /// `collections.namedtuple`
    #[strum(serialize = "namedtuple")]
    NamedTuple,
    /// `importlib.import_module`, which returns the submodule.
    ImportModule,
    /// `typing(_extensions).final`
    Final,
    /// `typing(_extensions).disjoint_base`
    DisjointBase,
    /// [`typing(_extensions).no_type_check`](https://typing.python.org/en/latest/spec/directives.html#no-type-check)
    NoTypeCheck,
    /// `typing(_extensions).type_check_only`
    TypeCheckOnly,

    /// `typing(_extensions).assert_type`
    AssertType,
    /// `typing(_extensions).assert_never`
    AssertNever,
    /// `typing(_extensions).cast`
    Cast,
    /// `typing(_extensions).overload`
    Overload,
    /// `typing(_extensions).override`
    Override,
    /// `typing(_extensions).is_protocol`
    IsProtocol,
    /// `typing(_extensions).get_protocol_members`
    GetProtocolMembers,
    /// `typing(_extensions).runtime_checkable`
    RuntimeCheckable,
    /// `typing(_extensions).dataclass_transform`
    DataclassTransform,

    /// `abc.abstractmethod`
    #[strum(serialize = "abstractmethod")]
    AbstractMethod,

    /// `contextlib.asynccontextmanager`
    #[strum(serialize = "asynccontextmanager")]
    AsyncContextManager,

    /// `dataclasses.dataclass`
    Dataclass,
    /// `dataclasses.field`
    Field,

    /// `inspect.getattr_static`
    GetattrStatic,

    /// `ty_extensions.static_assert`
    StaticAssert,
    /// `ty_extensions.is_equivalent_to`
    IsEquivalentTo,
    /// `ty_extensions.is_subtype_of`
    IsSubtypeOf,
    /// `ty_extensions.is_assignable_to`
    IsAssignableTo,
    /// `ty_extensions.is_disjoint_from`
    IsDisjointFrom,
    /// `ty_extensions.is_singleton`
    IsSingleton,
    /// `ty_extensions.is_single_valued`
    IsSingleValued,
    /// `ty_extensions.generic_context`
    GenericContext,
    /// `ty_extensions.into_callable`
    IntoCallable,
    /// `ty_extensions.dunder_all_names`
    DunderAllNames,
    /// `ty_extensions.enum_members`
    EnumMembers,
    /// `ty_extensions.all_members`
    AllMembers,
    /// `ty_extensions.has_member`
    HasMember,
    /// `ty_extensions.reveal_protocol_interface`
    RevealProtocolInterface,
    /// `ty_extensions.reveal_mro`
    RevealMro,
}

impl KnownFunction {
    pub fn into_classinfo_constraint_function(self) -> Option<ClassInfoConstraintFunction> {
        match self {
            Self::IsInstance => Some(ClassInfoConstraintFunction::IsInstance),
            Self::IsSubclass => Some(ClassInfoConstraintFunction::IsSubclass),
            _ => None,
        }
    }

    pub(crate) fn try_from_definition_and_name<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        name: &str,
    ) -> Option<Self> {
        let candidate = Self::from_str(name).ok()?;
        candidate
            .check_module(file_to_module(db, definition.file(db))?.known(db)?)
            .then_some(candidate)
    }

    /// Return `true` if `self` is defined in `module`
    const fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::IsInstance
            | Self::IsSubclass
            | Self::HasAttr
            | Self::Len
            | Self::Repr
            | Self::DunderImport => module.is_builtins(),
            Self::AssertType
            | Self::AssertNever
            | Self::Cast
            | Self::Overload
            | Self::Override
            | Self::RevealType
            | Self::Final
            | Self::IsProtocol
            | Self::GetProtocolMembers
            | Self::RuntimeCheckable
            | Self::DataclassTransform
            | Self::DisjointBase
            | Self::NoTypeCheck => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }
            Self::AbstractMethod => {
                matches!(module, KnownModule::Abc)
            }
            Self::AsyncContextManager => {
                matches!(module, KnownModule::Contextlib)
            }
            Self::Dataclass | Self::Field => {
                matches!(module, KnownModule::Dataclasses)
            }
            Self::GetattrStatic => module.is_inspect(),
            Self::IsAssignableTo
            | Self::IsDisjointFrom
            | Self::IsEquivalentTo
            | Self::IsSingleValued
            | Self::IsSingleton
            | Self::IsSubtypeOf
            | Self::GenericContext
            | Self::IntoCallable
            | Self::DunderAllNames
            | Self::EnumMembers
            | Self::StaticAssert
            | Self::HasMember
            | Self::RevealProtocolInterface
            | Self::RevealMro
            | Self::AllMembers => module.is_ty_extensions(),
            Self::ImportModule => module.is_importlib(),

            Self::TypeCheckOnly => matches!(module, KnownModule::Typing),
            Self::NamedTuple => matches!(module, KnownModule::Collections),
        }
    }

    /// Evaluate a call to this known function, and emit any diagnostics that are necessary
    /// as a result of the call.
    pub(super) fn check_call<'db>(
        self,
        context: &InferContext<'db, '_>,
        overload: &mut Binding<'db>,
        call_arguments: &CallArguments<'_, 'db>,
        call_expression: &ast::ExprCall,
        file: File,
    ) {
        let db = context.db();
        let parameter_types = overload.parameter_types();

        match self {
            KnownFunction::RevealType => {
                let revealed_type = overload
                    .arguments_for_parameter(call_arguments, 0)
                    .fold(UnionBuilder::new(db), |builder, (_, ty)| builder.add(ty))
                    .build();
                if let Some(builder) =
                    context.report_diagnostic(DiagnosticId::RevealedType, Severity::Info)
                {
                    let mut diag = builder.into_diagnostic("Revealed type");
                    let span = context.span(&call_expression.arguments.args[0]);
                    diag.annotate(Annotation::primary(span).message(format_args!(
                        "`{}`",
                        revealed_type
                            .display_with(db, DisplaySettings::default().preserve_long_unions())
                    )));
                }
            }

            KnownFunction::HasMember => {
                let [Some(ty), Some(Type::StringLiteral(member))] = parameter_types else {
                    return;
                };
                let ty_members = all_members(db, *ty);
                overload.set_return_type(Type::BooleanLiteral(
                    ty_members.iter().any(|m| m.name == member.value(db)),
                ));
            }

            KnownFunction::AssertType => {
                let [Some(actual_ty), Some(asserted_ty)] = parameter_types else {
                    return;
                };
                if actual_ty.is_equivalent_to(db, *asserted_ty) {
                    return;
                }
                if let Some(builder) = context.report_lint(&TYPE_ASSERTION_FAILURE, call_expression)
                {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Argument does not have asserted type `{}`",
                        asserted_ty.display(db),
                    ));

                    diagnostic.annotate(
                        Annotation::secondary(context.span(&call_expression.arguments.args[0]))
                            .message(format_args!("Inferred type is `{}`", actual_ty.display(db),)),
                    );

                    if actual_ty.is_subtype_of(db, *asserted_ty) {
                        diagnostic.info(format_args!(
                            "`{inferred_type}` is a subtype of `{asserted_type}`, but they are not equivalent",
                            asserted_type = asserted_ty.display(db),
                            inferred_type = actual_ty.display(db),
                        ));
                    } else {
                        diagnostic.info(format_args!(
                            "`{asserted_type}` and `{inferred_type}` are not equivalent types",
                            asserted_type = asserted_ty.display(db),
                            inferred_type = actual_ty.display(db),
                        ));
                    }

                    diagnostic.set_concise_message(format_args!(
                        "Type `{}` does not match asserted type `{}`",
                        asserted_ty.display(db),
                        actual_ty.display(db),
                    ));
                }
            }

            KnownFunction::AssertNever => {
                let [Some(actual_ty)] = parameter_types else {
                    return;
                };
                if actual_ty.is_equivalent_to(db, Type::Never) {
                    return;
                }
                if let Some(builder) = context.report_lint(&TYPE_ASSERTION_FAILURE, call_expression)
                {
                    let mut diagnostic =
                        builder.into_diagnostic("Argument does not have asserted type `Never`");
                    diagnostic.annotate(
                        Annotation::secondary(context.span(&call_expression.arguments.args[0]))
                            .message(format_args!(
                                "Inferred type of argument is `{}`",
                                actual_ty.display(db)
                            )),
                    );
                    diagnostic.info(format_args!(
                        "`Never` and `{inferred_type}` are not equivalent types",
                        inferred_type = actual_ty.display(db),
                    ));

                    diagnostic.set_concise_message(format_args!(
                        "Type `{}` is not equivalent to `Never`",
                        actual_ty.display(db),
                    ));
                }
            }

            KnownFunction::StaticAssert => {
                let [Some(parameter_ty), message] = parameter_types else {
                    return;
                };
                let truthiness = match parameter_ty.try_bool(db) {
                    Ok(truthiness) => truthiness,
                    Err(err) => {
                        let condition = call_expression
                            .arguments
                            .find_argument("condition", 0)
                            .map(|argument| match argument {
                                ruff_python_ast::ArgOrKeyword::Arg(expr) => {
                                    ast::AnyNodeRef::from(expr)
                                }
                                ruff_python_ast::ArgOrKeyword::Keyword(keyword) => {
                                    ast::AnyNodeRef::from(keyword)
                                }
                            })
                            .unwrap_or(ast::AnyNodeRef::from(call_expression));

                        err.report_diagnostic(context, condition);

                        return;
                    }
                };

                if let Some(builder) = context.report_lint(&STATIC_ASSERT_ERROR, call_expression) {
                    if truthiness.is_always_true() {
                        return;
                    }
                    let mut diagnostic = if let Some(message) = message
                        .and_then(Type::as_string_literal)
                        .map(|s| s.value(db))
                    {
                        builder.into_diagnostic(format_args!("Static assertion error: {message}"))
                    } else if *parameter_ty == Type::BooleanLiteral(false) {
                        builder.into_diagnostic(
                            "Static assertion error: argument evaluates to `False`",
                        )
                    } else if truthiness.is_always_false() {
                        builder.into_diagnostic(format_args!(
                            "Static assertion error: argument of type `{parameter_ty}` \
                            is statically known to be falsy",
                            parameter_ty = parameter_ty.display(db)
                        ))
                    } else {
                        builder.into_diagnostic(format_args!(
                            "Static assertion error: argument of type `{parameter_ty}` \
                            has an ambiguous static truthiness",
                            parameter_ty = parameter_ty.display(db)
                        ))
                    };
                    diagnostic.annotate(
                        Annotation::secondary(context.span(&call_expression.arguments.args[0]))
                            .message(format_args!(
                                "Inferred type of argument is `{}`",
                                parameter_ty.display(db)
                            )),
                    );
                }
            }

            KnownFunction::Cast => {
                let [Some(casted_type), Some(source_type)] = parameter_types else {
                    return;
                };
                let contains_unknown_or_todo =
                    |ty| matches!(ty, Type::Dynamic(dynamic) if dynamic != DynamicType::Any);
                if source_type.is_equivalent_to(db, *casted_type)
                    && !any_over_type(db, *source_type, &contains_unknown_or_todo, true)
                    && !any_over_type(db, *casted_type, &contains_unknown_or_todo, true)
                {
                    if let Some(builder) = context.report_lint(&REDUNDANT_CAST, call_expression) {
                        let source_display = source_type.display(db).to_string();
                        let casted_display = casted_type.display(db).to_string();
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Value is already of type `{casted_display}`",
                        ));
                        if source_display != casted_display {
                            diagnostic.info(format_args!(
                                "`{casted_display}` is equivalent to `{source_display}`",
                            ));
                        }
                    }
                }
            }

            KnownFunction::GetProtocolMembers => {
                let [Some(Type::ClassLiteral(class))] = parameter_types else {
                    return;
                };
                if class.is_protocol(db) {
                    return;
                }
                report_bad_argument_to_get_protocol_members(context, call_expression, *class);
            }

            KnownFunction::RevealProtocolInterface => {
                let [Some(param_type)] = parameter_types else {
                    return;
                };
                let Some(protocol_class) = param_type
                    .to_class_type(db)
                    .and_then(|class| class.into_protocol_class(db))
                else {
                    report_bad_argument_to_protocol_interface(
                        context,
                        call_expression,
                        *param_type,
                    );
                    return;
                };
                if let Some(builder) =
                    context.report_diagnostic(DiagnosticId::RevealedType, Severity::Info)
                {
                    let mut diag = builder.into_diagnostic("Revealed protocol interface");
                    let span = context.span(&call_expression.arguments.args[0]);
                    diag.annotate(Annotation::primary(span).message(format_args!(
                        "`{}`",
                        protocol_class.interface(db).display(db)
                    )));
                }
            }

            KnownFunction::RevealMro => {
                let [Some(param_type)] = parameter_types else {
                    return;
                };
                let mut good_argument = true;
                let classes = match param_type {
                    Type::ClassLiteral(class) => vec![ClassType::NonGeneric(*class)],
                    Type::GenericAlias(generic_alias) => vec![ClassType::Generic(*generic_alias)],
                    Type::Union(union) => {
                        let elements = union.elements(db);
                        let mut classes = Vec::with_capacity(elements.len());
                        for element in elements {
                            match element {
                                Type::ClassLiteral(class) => {
                                    classes.push(ClassType::NonGeneric(*class));
                                }
                                Type::GenericAlias(generic_alias) => {
                                    classes.push(ClassType::Generic(*generic_alias));
                                }
                                _ => {
                                    good_argument = false;
                                    break;
                                }
                            }
                        }
                        classes
                    }
                    _ => {
                        good_argument = false;
                        vec![]
                    }
                };
                if !good_argument {
                    let Some(builder) =
                        context.report_lint(&INVALID_ARGUMENT_TYPE, call_expression)
                    else {
                        return;
                    };
                    let mut diagnostic =
                        builder.into_diagnostic("Invalid argument to `reveal_mro`");
                    diagnostic.set_primary_message(format_args!(
                        "Can only pass a class object, generic alias or a union thereof"
                    ));
                    return;
                }
                if let Some(builder) =
                    context.report_diagnostic(DiagnosticId::RevealedType, Severity::Info)
                {
                    let mut diag = builder.into_diagnostic("Revealed MRO");
                    let span = context.span(&call_expression.arguments.args[0]);
                    let mut message = String::new();
                    for (i, class) in classes.iter().enumerate() {
                        message.push('(');
                        for class in class.iter_mro(db) {
                            message.push_str(&class.display(db).to_string());
                            // Omit the comma for the last element (which is always `object`)
                            if class
                                .into_class()
                                .is_none_or(|base| !base.is_object(context.db()))
                            {
                                message.push_str(", ");
                            }
                        }
                        // If the last element was also the first element
                        // (i.e., it's a length-1 tuple -- which can only happen if we're revealing
                        // the MRO for `object` itself), add a trailing comma so that it's still a
                        // valid tuple display.
                        if class.is_object(db) {
                            message.push(',');
                        }
                        message.push(')');
                        if i < classes.len() - 1 {
                            message.push_str(" | ");
                        }
                    }
                    diag.annotate(Annotation::primary(span).message(message));
                }
            }

            KnownFunction::IsInstance | KnownFunction::IsSubclass => {
                let [Some(first_arg), Some(second_argument)] = parameter_types else {
                    return;
                };

                match second_argument {
                    Type::ClassLiteral(class) => {
                        if let Some(protocol_class) = class.into_protocol_class(db)
                            && !protocol_class.is_runtime_checkable(db)
                        {
                            report_runtime_check_against_non_runtime_checkable_protocol(
                                context,
                                call_expression,
                                protocol_class,
                                self,
                            );
                        }

                        if self == KnownFunction::IsInstance {
                            overload.set_return_type(
                                is_instance_truthiness(db, *first_arg, *class).into_type(db),
                            );
                        }
                    }
                    // The special-casing here is necessary because we recognise the symbol `typing.Any` as an
                    // instance of `type` at runtime. Even once we understand typeshed's annotation for
                    // `isinstance()`, we'd continue to accept calls such as `isinstance(x, typing.Any)` without
                    // emitting a diagnostic if we didn't have this branch.
                    Type::SpecialForm(SpecialFormType::Any)
                        if self == KnownFunction::IsInstance =>
                    {
                        let Some(builder) =
                            context.report_lint(&INVALID_ARGUMENT_TYPE, call_expression)
                        else {
                            return;
                        };
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "`typing.Any` cannot be used with `isinstance()`"
                        ));
                        diagnostic
                            .set_primary_message("This call will raise `TypeError` at runtime");
                    }

                    Type::KnownInstance(KnownInstanceType::UnionType(_)) => {
                        fn find_invalid_elements<'db>(
                            db: &'db dyn Db,
                            function: KnownFunction,
                            ty: Type<'db>,
                            invalid_elements: &mut Vec<Type<'db>>,
                        ) {
                            match ty {
                                Type::ClassLiteral(_) => {}
                                Type::NominalInstance(instance)
                                    if instance.has_known_class(db, KnownClass::NoneType) => {}
                                Type::SpecialForm(special_form)
                                    if special_form.is_valid_isinstance_target() => {}
                                // `Any` can be used in `issubclass()` calls but not `isinstance()` calls
                                Type::SpecialForm(SpecialFormType::Any)
                                    if function == KnownFunction::IsSubclass => {}
                                Type::KnownInstance(KnownInstanceType::UnionType(instance)) => {
                                    match instance.value_expression_types(db) {
                                        Ok(value_expression_types) => {
                                            for element in value_expression_types {
                                                find_invalid_elements(
                                                    db,
                                                    function,
                                                    element,
                                                    invalid_elements,
                                                );
                                            }
                                        }
                                        Err(_) => {
                                            invalid_elements.push(ty);
                                        }
                                    }
                                }
                                _ => invalid_elements.push(ty),
                            }
                        }

                        let mut invalid_elements = vec![];
                        find_invalid_elements(db, self, *second_argument, &mut invalid_elements);

                        let Some((first_invalid_element, other_invalid_elements)) =
                            invalid_elements.split_first()
                        else {
                            return;
                        };

                        let Some(builder) =
                            context.report_lint(&INVALID_ARGUMENT_TYPE, call_expression)
                        else {
                            return;
                        };

                        let function_name: &str = self.into();

                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Invalid second argument to `{function_name}`"
                        ));
                        diagnostic.info(format_args!(
                            "A `UnionType` instance can only be used as the second argument to \
                            `{function_name}` if all elements are class objects"
                        ));
                        diagnostic.annotate(
                            Annotation::secondary(context.span(&call_expression.arguments.args[1]))
                                .message("This `UnionType` instance contains non-class elements"),
                        );
                        match other_invalid_elements {
                            [] => diagnostic.info(format_args!(
                                "Element `{}` in the union is not a class object",
                                first_invalid_element.display(db)
                            )),
                            [single] => diagnostic.info(format_args!(
                                "Elements `{}` and `{}` in the union are not class objects",
                                first_invalid_element.display(db),
                                single.display(db),
                            )),
                            _ => diagnostic.info(format_args!(
                                "Element `{}` in the union, and {} more elements, are not class objects",
                                first_invalid_element.display(db),
                                other_invalid_elements.len(),
                            ))
                        }
                    }
                    _ => {}
                }
            }

            known @ (KnownFunction::DunderImport | KnownFunction::ImportModule) => {
                let [Some(Type::StringLiteral(full_module_name)), rest @ ..] = parameter_types
                else {
                    return;
                };

                if rest.iter().any(Option::is_some) {
                    return;
                }

                let module_name = full_module_name.value(db);

                if known == KnownFunction::DunderImport && module_name.contains('.') {
                    // `__import__("collections.abc")` returns the `collections` module.
                    // `importlib.import_module("collections.abc")` returns the `collections.abc` module.
                    // ty doesn't have a way to represent the return type of the former yet.
                    // https://github.com/astral-sh/ruff/pull/19008#discussion_r2173481311
                    return;
                }

                let Some(module_name) = ModuleName::new(module_name) else {
                    return;
                };
                let Some(module) = resolve_module(db, file, &module_name) else {
                    return;
                };

                overload.set_return_type(Type::module_literal(db, file, module));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use strum::IntoEnumIterator;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::place::known_module_symbol;

    #[test]
    fn known_function_roundtrip_from_str() {
        let db = setup_db();

        for function in KnownFunction::iter() {
            let function_name: &'static str = function.into();

            let module = match function {
                KnownFunction::Len
                | KnownFunction::Repr
                | KnownFunction::IsInstance
                | KnownFunction::HasAttr
                | KnownFunction::IsSubclass
                | KnownFunction::DunderImport => KnownModule::Builtins,

                KnownFunction::AbstractMethod => KnownModule::Abc,

                KnownFunction::AsyncContextManager => KnownModule::Contextlib,

                KnownFunction::Dataclass | KnownFunction::Field => KnownModule::Dataclasses,

                KnownFunction::GetattrStatic => KnownModule::Inspect,

                KnownFunction::Cast
                | KnownFunction::Final
                | KnownFunction::Overload
                | KnownFunction::Override
                | KnownFunction::RevealType
                | KnownFunction::AssertType
                | KnownFunction::AssertNever
                | KnownFunction::IsProtocol
                | KnownFunction::GetProtocolMembers
                | KnownFunction::RuntimeCheckable
                | KnownFunction::DataclassTransform
                | KnownFunction::DisjointBase
                | KnownFunction::NoTypeCheck => KnownModule::TypingExtensions,

                KnownFunction::TypeCheckOnly => KnownModule::Typing,

                KnownFunction::IsSingleton
                | KnownFunction::IsSubtypeOf
                | KnownFunction::GenericContext
                | KnownFunction::IntoCallable
                | KnownFunction::DunderAllNames
                | KnownFunction::EnumMembers
                | KnownFunction::StaticAssert
                | KnownFunction::IsDisjointFrom
                | KnownFunction::IsSingleValued
                | KnownFunction::IsAssignableTo
                | KnownFunction::IsEquivalentTo
                | KnownFunction::HasMember
                | KnownFunction::RevealProtocolInterface
                | KnownFunction::RevealMro
                | KnownFunction::AllMembers => KnownModule::TyExtensions,

                KnownFunction::ImportModule => KnownModule::ImportLib,
                KnownFunction::NamedTuple => KnownModule::Collections,
            };

            let function_definition = known_module_symbol(&db, module, function_name)
                .place
                .expect_type()
                .expect_function_literal()
                .definition(&db);

            assert_eq!(
                KnownFunction::try_from_definition_and_name(
                    &db,
                    function_definition,
                    function_name
                ),
                Some(function),
                "The strum `EnumString` implementation appears to be incorrect for `{function_name}`"
            );
        }
    }
}
