//! Infer assignment values as the unpacker matches them to their targets.
//!
//! A target's declaration or setter supplies context to the expression assigned to it. The
//! unpacker identifies that expression before asking this builder to infer it, and then builds
//! the enclosing tuple or list from the inferred elements.

use std::cell::{Cell, OnceCell};

use ruff_python_ast as ast;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use ty_python_core::ExpressionNodeKey;
use ty_python_core::definition::Definition;
use ty_python_core::expression::Expression;
use ty_python_core::unpack::Unpack;

use super::{AddBinding, CollectionElement, TypeInferenceBuilder};
use crate::Db;
use crate::types::infer::ExpressionInference;
use crate::types::tuple::{TupleElement, TupleLength};
use crate::types::unpacker::{
    UnpackAssignedValue, UnpackCaptured, UnpackInference, UnpackResult, UnpackValueInference,
    Unpacker, literal_source_expressions, sequence_elts, target_length,
};
use crate::types::{KnownClass, Type, TypeContext, UnionBuilder, infer_expression_types};

// Most unpacking assignments have four or fewer values.
const COMMON_UNPACK_LENGTH: usize = 4;
type UnpackElts<T> = SmallVec<[T; COMMON_UNPACK_LENGTH]>;

struct AssignmentInference<'builder, 'db, 'ast> {
    builder: &'builder mut TypeInferenceBuilder<'db, 'ast>,
    value: &'ast ast::Expr,
    contextual_expressions: FxHashSet<ExpressionNodeKey>,
    validated_targets: FxHashSet<ExpressionNodeKey>,
    /// Bindings for unpacked targets, separate from bindings made while evaluating the source:
    ///
    /// ```python
    /// first, second = ((third := 1), 2)
    /// ```
    ///
    /// The shared source inference owns `third`; this result owns `first` and `second`.
    binding_targets: UnpackElts<(Definition<'db>, Type<'db>)>,
    /// Cached declaration lookups for name targets. Looking up a name again when validating its
    /// matched value would report conflicting declarations twice for the same assignment.
    name_bindings: FxHashMap<ExpressionNodeKey, AddBinding<'db, 'ast>>,
    needs_value_inference: bool,
    prior_contexts: &'builder FxHashMap<ExpressionNodeKey, TypeContext<'db>>,
    source_contexts: FxHashMap<ExpressionNodeKey, TypeContext<'db>>,
    /// The ordinary source inference shared by targets in the first attempt when a write
    /// selects no context.
    shared_source: Option<&'builder SharedSource<'db>>,
    /// Contexts selected by writes after another target has used the shared ordinary source.
    /// Another attempt then infers the source with those contexts.
    observed_contexts: Option<FxHashMap<ExpressionNodeKey, TypeContext<'db>>>,
}

#[derive(Debug)]
struct UnpackAttempt<'db> {
    result: UnpackResult<'db>,
    observed_contexts: Option<FxHashMap<ExpressionNodeKey, TypeContext<'db>>>,
}

/// The ordinary source inference shared between the targets of one assignment.
/// It is computed only when a target needs a source value without selecting context for it:
///
/// ```python
/// first, second = third, fourth = make_values()
/// ```
///
/// Both targets use the same inference of `make_values()` unless a write selects context.
#[derive(Debug)]
struct SharedSource<'db> {
    expression: Expression<'db>,
    inference: OnceCell<&'db ExpressionInference<'db>>,
}

impl<'db> SharedSource<'db> {
    fn new(expression: Expression<'db>) -> Self {
        Self {
            expression,
            inference: OnceCell::new(),
        }
    }

    fn get(&self, db: &'db dyn Db) -> &'db ExpressionInference<'db> {
        self.inference
            .get_or_init(|| infer_expression_types(db, self.expression, TypeContext::default()))
    }

    fn inferred(&self) -> Option<&'db ExpressionInference<'db>> {
        self.inference.get().copied()
    }
}

/// The write context selected by the current builder during assignment validation.
/// A setter may try several value contexts through nested speculative inference:
///
/// ```python
/// from typing import Literal, overload
///
/// class Container:
///     @overload
///     def __setitem__(self, key: Literal[0], value: list[int]) -> None: ...
///     @overload
///     def __setitem__(self, key: Literal[1], value: list[str]) -> None: ...
///     def __setitem__(self, key: int, value: list[int] | list[str]) -> None: ...
///
/// container = Container()
/// container[0], other = ([], 0)
/// ```
///
/// Only the context used by the final write belongs to the current builder.
#[derive(Debug)]
struct ObservedWriteContext<'db> {
    depth: usize,
    selected: Cell<Option<TypeContext<'db>>>,
}

impl<'db> ObservedWriteContext<'db> {
    fn new(builder: &TypeInferenceBuilder<'db, '_>) -> Self {
        Self {
            depth: builder.speculation_depth,
            selected: Cell::new(None),
        }
    }

    fn record(&self, builder: &TypeInferenceBuilder<'db, '_>, tcx: TypeContext<'db>) {
        if builder.speculation_depth == self.depth {
            self.selected.set(Some(tcx));
        }
    }
}

/// Infer the list created by a capture from elements matched by the unpacker.
fn infer_matched_list<'db, 'ast>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    elements: impl Iterator<Item = CollectionElement<'db, 'ast>>,
    tcx: TypeContext<'db>,
) -> Type<'db> {
    let elts: UnpackElts<[Option<CollectionElement<'db, 'ast>>; 1]> =
        elements.map(|element| [Some(element)]).collect();
    builder
        .infer_collection_literal(
            KnownClass::List,
            None,
            &elts,
            &mut |builder, (_, element, tcx)| builder.infer_expression(element, tcx),
            tcx,
        )
        .unwrap_or_else(Type::unknown)
}

fn infer_source_expression<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    shared_source: Option<&SharedSource<'db>>,
    expression: &ast::Expr,
    tcx: TypeContext<'db>,
    source_contexts: &FxHashMap<ExpressionNodeKey, TypeContext<'db>>,
) -> Type<'db> {
    if let Some(source) = shared_source
        && tcx.annotation.is_none()
    {
        source.get(builder.db()).expression_type(expression)
    } else if source_contexts.is_empty() {
        builder.infer_expression_impl(expression, tcx)
    } else {
        infer_source_with_matched_contexts(builder, expression, tcx, source_contexts)
    }
}

/// Infer an assignment's source in evaluation order, reusing the ordinary tuple, list, and
/// starred-expression inference while applying context to expressions matched by the unpacker.
///
/// ```python
/// def assign(existing: list[list[object]]) -> None:
///     rest: list[list[object]]
///     first, *rest = (0, *existing, [1])
/// ```
///
/// Infer `[1]` with `list[object]` context before constructing the enclosing tuple. The
/// elements of `existing` have already been inferred and keep their original types.
fn infer_source_with_matched_contexts<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    expression: &ast::Expr,
    tcx: TypeContext<'db>,
    source_contexts: &FxHashMap<ExpressionNodeKey, TypeContext<'db>>,
) -> Type<'db> {
    if let Some(ty) = builder.expressions.get(&expression.into()) {
        return *ty;
    }
    if let Some(tcx) = source_contexts.get(&expression.into()) {
        return builder.infer_expression_impl(expression, *tcx);
    }

    let ty = match expression {
        ast::Expr::Tuple(tuple) => {
            builder.infer_tuple_expression_with(tuple, tcx, &mut |builder, element, tcx| {
                infer_source_with_matched_contexts(builder, element, tcx, source_contexts)
            })
        }
        ast::Expr::List(list) => {
            let elts: UnpackElts<[Option<&ast::Expr>; 1]> =
                list.elts.iter().map(|element| [Some(element)]).collect();
            builder
                .infer_collection_literal(
                    KnownClass::List,
                    Some(list.into()),
                    &elts,
                    &mut |builder, (_, element, tcx)| {
                        infer_source_with_matched_contexts(builder, element, tcx, source_contexts)
                    },
                    tcx,
                )
                // Custom typesheds may omit list or define it without type parameters.
                .unwrap_or_else(Type::unknown)
        }
        ast::Expr::Starred(starred) => {
            infer_source_with_matched_contexts(builder, &starred.value, tcx, source_contexts);
            return builder.infer_expression_impl(expression, tcx);
        }
        _ => return builder.infer_expression_impl(expression, tcx),
    };
    builder.store_expression_type(expression, ty);
    ty
}

/// Match source expressions before contextual inference. Only expressions that always go to
/// the same target receive its context; an existing iterable contributes types, not expressions.
///
/// ```python
/// def assign(existing: list[list[object]]) -> None:
///     rest: list[list[object]]
///     first, *rest = (0, *existing, [1])
/// ```
///
/// The literal `0` always fills `first`, so `[1]` always enters `rest`. Without `0`, an empty
/// `existing` would let `[1]` fill `first`, so the capture could not supply its context.
fn matched_source_contexts<'db>(
    db: &'db dyn Db,
    env: &crate::ProgramEnvironment<'db>,
    target: &ast::Expr,
    value: &ast::Expr,
    target_contexts: &FxHashMap<ExpressionNodeKey, TypeContext<'db>>,
    source_contexts: &mut FxHashMap<ExpressionNodeKey, TypeContext<'db>>,
) {
    let Some(targets) = sequence_elts(target) else {
        return;
    };
    let Some(source) = literal_source_expressions(value) else {
        return;
    };
    let length = target_length(targets);
    let captures_are_certain = match (source.len(), length) {
        (
            TupleLength::Variable(source_prefix, source_suffix),
            TupleLength::Variable(target_prefix, target_suffix),
        ) => source_prefix >= target_prefix && source_suffix >= target_suffix,
        _ => true,
    };
    let Ok(matched) = source.unpack(length, Clone::clone, |_| None) else {
        return;
    };

    for (target, element) in targets.iter().zip(matched.into_all_elements_with_kind()) {
        match element {
            TupleElement::Fixed(Some(expression))
            | TupleElement::Prefix(Some(expression))
            | TupleElement::Suffix(Some(expression)) => {
                if sequence_elts(target).is_some() {
                    matched_source_contexts(
                        db,
                        env,
                        target,
                        expression,
                        target_contexts,
                        source_contexts,
                    );
                } else if let Some(context) = target_contexts.get(&target.into()) {
                    source_contexts.insert(expression.into(), *context);
                }
            }
            TupleElement::Variable(expressions) if captures_are_certain => {
                if let ast::Expr::Starred(starred) = target
                    && let Some(element_type) = target_contexts
                        .get(&starred.value.as_ref().into())
                        .and_then(|tcx| tcx.annotation)
                        .and_then(|ty| ty.try_iterate(db, env).ok())
                        .map(|elements| elements.homogeneous_element_type(db, env))
                {
                    for expression in expressions.into_iter().flatten() {
                        source_contexts
                            .insert(expression.into(), TypeContext::new(Some(element_type)));
                    }
                }
            }
            _ => {}
        }
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer and unpack an assignment's right-hand side with context from its matched targets.
    ///
    /// A starred target receives a new list with no corresponding list-literal expression:
    ///
    /// ```python
    /// rest: list[object]
    /// first, *rest = (0, 1, 2)
    /// ```
    ///
    /// The returned `UnpackResult` records `list[object]` for the fresh list assigned to
    /// `rest`, as well as the types of the original source expressions `0`, `1`, and `2`.
    ///
    /// When a call supplies the entire tuple, match its inferred return type to the targets
    /// first, then infer the call with the contexts selected by those target writes:
    ///
    /// ```python
    /// def pair[T]() -> tuple[list[T], T]:
    ///     raise NotImplementedError
    ///
    /// values: list[int]
    /// values, element = pair()
    /// ```
    ///
    /// Here the declaration of `values` lets the second inference solve `T` as `int`.
    /// This additional inference is needed only when the source does not have separate
    /// expressions to match with its targets. A write's context can depend on a value
    /// returned by that same source:
    ///
    /// ```python
    /// from typing import Literal, overload
    ///
    /// class Container:
    ///     @overload
    ///     def __setitem__(self, key: Literal[1], value: list[int]) -> None: ...
    ///     @overload
    ///     def __setitem__(self, key: Literal[2], value: list[str]) -> None: ...
    ///     def __setitem__(self, key: int, value: list[int] | list[str]) -> None: ...
    ///
    /// def make_pair[T]() -> tuple[Literal[1], list[T]]:
    ///     raise NotImplementedError
    ///
    /// container = Container()
    /// key, container[key] = make_pair()
    /// ```
    ///
    /// We must infer `key` from `make_pair()` before the ordinary subscript write can
    /// select `list[int]`; that selected context then specializes `make_pair()`.
    /// Each attempt uses the same unpacking and target-write path. There is no separate
    /// implementation that looks up declarations or setter signatures in advance.
    pub(in crate::types::infer) fn finish_unpack(self, unpack: Unpack<'db>) -> UnpackResult<'db> {
        let source = SharedSource::new(unpack.value(self.db()).expression());
        // The ordinary source is shared between the targets of a chained assignment:
        //
        //     first, second = third, fourth = make_values()
        //
        // Match each target and validate its writes using that source. If a write selects
        // context, the later attempt infers the source through those same writes instead.
        let retry_builder = self.speculate();
        let ordinary = self.run_unpack(unpack, &FxHashMap::default(), Some(&source));
        let Some(mut contexts) = ordinary
            .observed_contexts
            .filter(|contexts| !contexts.is_empty())
        else {
            return ordinary.result;
        };

        // A target can choose a different expected type once a previous target has
        // specialized the source. Retain only expectations that the actual write still
        // selects. The same check handles declarations and setters, including cases
        // where a setter's key reads an earlier binding from this assignment.
        loop {
            // A later pass may reuse cached expression results from this trial, so
            // preserve any diagnostics attached to those results.
            let attempt = retry_builder
                .speculate()
                .run_unpack(unpack, &contexts, None);
            let Some(observed) = attempt.observed_contexts else {
                // A literal source was matched and inferred through its individual writes;
                // it needs no expectation for the enclosing expression.
                return attempt.result;
            };
            let previous_len = contexts.len();
            contexts.retain(|target, tcx| observed.get(target) == Some(tcx));

            if contexts.is_empty() {
                // The first pass already validated the unconstrained assignment.
                return ordinary.result;
            }
            if contexts.len() == previous_len {
                // This attempt used precisely the contexts the writes selected. Its
                // types and diagnostics are final; there is no need to infer it again.
                return attempt.result;
            }
            // Each retry removes at least one context, so this loop terminates.
        }
    }

    fn run_unpack(
        mut self,
        unpack: Unpack<'db>,
        prior_contexts: &FxHashMap<ExpressionNodeKey, TypeContext<'db>>,
        shared_source: Option<&SharedSource<'db>>,
    ) -> UnpackAttempt<'db> {
        let db = self.db();
        let env = self.program_environment();
        let module = self.module();
        let target = unpack.target(db, module);
        let value = unpack.value(db);
        let value_expr = value.expression().node_ref(db).node(module);
        let mut source_contexts = FxHashMap::default();
        if !prior_contexts.is_empty() {
            matched_source_contexts(
                db,
                env,
                target,
                value_expr,
                prior_contexts,
                &mut source_contexts,
            );
        }

        // Field specifiers assigned through unpacking still contribute metadata to the
        // generated dataclass constructor:
        //
        //     from dataclasses import dataclass, field
        //
        //     @dataclass
        //     class Example:
        //         value: int
        //         value, other = field(default=1, init=False), 0
        //
        // Without this setup, inferring field(...) here would lose init=False.
        self.setup_dataclass_field_specifiers();

        let mut inference = AssignmentInference {
            builder: &mut self,
            value: value_expr,
            contextual_expressions: FxHashSet::default(),
            validated_targets: FxHashSet::default(),
            binding_targets: SmallVec::new(),
            name_bindings: FxHashMap::default(),
            needs_value_inference: false,
            prior_contexts,
            source_contexts,
            shared_source,
            // If this attempt uses the shared ordinary source, context selected by
            // its writes requires another attempt to infer that source with context.
            observed_contexts: shared_source.map(|_| FxHashMap::default()),
        };
        let mut unpacker = Unpacker::new(
            db,
            env,
            unpack.target_scope(db),
            unpack.program_file(db),
            module,
        );
        unpacker.unpack(target, value, &mut inference);
        let needs_value_inference = inference.needs_value_inference;
        let inferred_source = shared_source.and_then(SharedSource::inferred);
        let observed = inferred_source.and(inference.observed_contexts);
        let binding_targets = inference.binding_targets;
        let retain_inference = needs_value_inference || self.context.has_diagnostics();
        let value_inference = if retain_inference {
            let mut externally_owned: FxHashSet<_> = binding_targets
                .iter()
                .map(|(definition, _)| *definition)
                .collect();
            if let Some(source) = inferred_source {
                // Both this attempt and the shared source can infer a binding inside
                // the source, such as `saved` in `first, second = (saved := 1), 2`.
                // The shared source owns that binding; the unpack result owns the
                // targets. Keep each only once when combining the inference results.
                if let Some(extra) = source.extra.as_deref() {
                    externally_owned
                        .extend(extra.bindings.iter().map(|(definition, _)| *definition));
                }
            }
            self.bindings
                .retain(|(definition, _)| !externally_owned.contains(definition));
            let local = self.into_expression_inference();
            Some(if inferred_source.is_some() {
                UnpackValueInference::SharedWithWrites(local)
            } else {
                UnpackValueInference::Contextual(local)
            })
        } else {
            self.context.defuse();
            None
        };
        let unpacked = unpacker.finish().with_bindings(binding_targets);
        let result = if let Some(inference) = value_inference {
            unpacked.with_value_inference(inference)
        } else {
            unpacked
        };
        UnpackAttempt {
            result,
            observed_contexts: observed,
        }
    }
}

impl<'db, 'ast> AssignmentInference<'_, 'db, 'ast> {
    /// A name's declaration supplies context without inferring the definition's value.
    ///
    /// ```python
    /// values: list[object]
    /// values, other = ([1], 0)
    /// ```
    ///
    /// The value may itself read its target:
    ///
    /// ```python
    /// def update(flag: bool):
    ///     rest: list[int] = [1]
    ///     while flag:
    ///         first, *rest = (rest[0], 1, 2)
    /// ```
    ///
    /// Asking for `rest`'s inferred value here would recurse into this unpacking query.
    /// Member targets use their ordinary write path so setters choose the accepted context.
    fn name_type_context(&mut self, target: &'ast ast::Expr) -> TypeContext<'db> {
        let ast::Expr::Name(name) = target else {
            return TypeContext::default();
        };
        let key = target.into();
        let tcx = if let Some(binding) = self.name_bindings.get(&key) {
            binding.type_context()
        } else if let Some(definition) = self.builder.index.try_definition(name) {
            let binding = self.builder.add_binding(target.into(), definition);
            let tcx = binding.type_context();
            self.name_bindings.insert(key, binding);
            tcx
        } else {
            TypeContext::default()
        };
        self.needs_value_inference |= tcx.annotation.is_some();
        self.observe_context(target, tcx);
        tcx
    }

    fn observe_context(&mut self, target: &ast::Expr, tcx: TypeContext<'db>) {
        if tcx.annotation.is_some()
            && let Some(observed_contexts) = self.observed_contexts.as_mut()
        {
            observed_contexts.insert(target.into(), tcx);
        }
    }

    /// Infer and validate a member target using the ordinary assignment path. The callback
    /// can infer a matched expression, construct a captured list, or return a value that
    /// was already inferred; in each case the setter selects its context in the same way.
    fn infer_member_target(
        &mut self,
        target: &ast::Expr,
        value: &ast::Expr,
        infer_assigned_ty: &dyn Fn(
            &mut TypeInferenceBuilder<'db, 'ast>,
            TypeContext<'db>,
        ) -> Type<'db>,
    ) -> Option<Type<'db>> {
        self.needs_value_inference = true;
        let observed = ObservedWriteContext::new(self.builder);
        self.builder.infer_target_impl(
            target,
            value,
            Some(&|builder, tcx| {
                observed.record(builder, tcx);
                infer_assigned_ty(builder, tcx)
            }),
        );
        self.validated_targets.insert(target.into());
        self.observe_context(target, observed.selected.get().unwrap_or_default());
        self.builder.try_expression_type(target)
    }

    /// Infer each fresh list using the context selected by its destination.
    ///
    /// ```python
    /// def assign(source: tuple[int] | tuple[str]) -> None:
    ///     rest: list[int | str]
    ///     (*rest,) = source
    /// ```
    ///
    /// Each source alternative constructs a separate list before their types are joined.
    /// An existing iterable supplies inferred element types; its elements are not copied
    /// or widened, although the outer captured list is new.
    fn infer_captured_list(
        &mut self,
        target: &'ast ast::Expr,
        source: UnpackCaptured<'_, 'db, 'ast>,
    ) -> Option<Type<'db>> {
        let db = self.builder.db();
        let env = self.builder.program_environment();
        let shared_source = self.shared_source;
        let infer_lists = |builder: &mut TypeInferenceBuilder<'db, 'ast>, tcx: TypeContext<'db>| {
            let mut lists = UnionBuilder::new(db, env).unpack_aliases(false);
            match source {
                UnpackCaptured::Expressions(elements) => {
                    // Each matched literal has an expression to infer with the write's
                    // context, or to look up in the shared source when unconstrained.
                    lists.add_in_place(infer_matched_list(
                        builder,
                        elements.iter().flatten().copied().map(|element| {
                            if let Some(source) = shared_source
                                && tcx.annotation.is_none()
                            {
                                CollectionElement::Inferred {
                                    ty: source.get(db).expression_type(element),
                                    expression: Some(element),
                                }
                            } else {
                                CollectionElement::Expression(element)
                            }
                        }),
                        tcx,
                    ));
                }
                UnpackCaptured::Alternatives(alternatives) => {
                    for elements in alternatives {
                        // Retain literal syntax for tuple-size promotion. An element taken
                        // from an existing iterable has no expression to promote.
                        lists.add_in_place(infer_matched_list(
                            builder,
                            elements.iter().map(|element| CollectionElement::Inferred {
                                ty: element.ty,
                                expression: element.expression,
                            }),
                            tcx,
                        ));
                    }
                }
            }
            lists.build()
        };

        match target {
            ast::Expr::Name(_) => {
                let tcx = self.name_type_context(target);
                tcx.annotation.map(|_| infer_lists(self.builder, tcx))
            }
            ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                // The setter sees the union of fresh lists, with each alternative
                // constructed under the write context it selects:
                //
                //     class Container:
                //         def __setitem__(self, key: int, value: list[int | str]) -> None: ...
                //
                //     def assign(container: Container, source: tuple[int] | tuple[str]):
                //         (*container[0],) = source
                self.infer_member_target(target, self.value, &infer_lists)
            }
            _ => None,
        }
    }
}

impl<'db, 'ast> UnpackInference<'db, 'ast> for AssignmentInference<'_, 'db, 'ast> {
    fn expression_type(&self, expression: &ast::Expr) -> Type<'db> {
        self.builder
            .try_expression_type(expression)
            .or_else(|| {
                self.shared_source
                    .and_then(SharedSource::inferred)
                    .map(|source| source.expression_type(expression))
            })
            .or_else(|| self.builder.fallback_type())
            .unwrap_or_else(Type::unknown)
    }

    fn has_inferred_expression(&self, expression: &ast::Expr) -> bool {
        self.builder.expressions.contains_key(&expression.into())
    }

    fn infer_enclosing_expression(&mut self, expression: &'ast ast::Expr) {
        if self
            .shared_source
            .and_then(SharedSource::inferred)
            .is_some()
        {
            // The ordinary source already records this enclosing expression. If a target
            // supplies context, another attempt reconstructs it from contextual elements.
            return;
        }
        // Matched elements have already received their own contexts. Build the enclosing
        // expression from those types, including any literal expansions within it.
        infer_source_with_matched_contexts(
            self.builder,
            expression,
            TypeContext::default(),
            &self.source_contexts,
        );
    }

    fn target_type_context(&mut self, target: &'ast ast::Expr) -> TypeContext<'db> {
        // When the source has no element expression for each target, infer its full
        // type before matching its values to the targets. The selected write contexts
        // from that match can then guide inference of the full source expression.
        self.observed_contexts.get_or_insert_default();
        let tcx = self
            .prior_contexts
            .get(&target.into())
            .copied()
            .unwrap_or_default();
        self.needs_value_inference |= tcx.annotation.is_some();
        tcx
    }

    fn capture_needs_context(&mut self, target: &'ast ast::Expr) -> bool {
        match target {
            // Without a declaration, the inferred list has no context to apply.
            // For `(*rest,) = source`, retain source alternatives only if `rest`
            // has an annotation that can affect the newly created list.
            ast::Expr::Name(_) => self.name_type_context(target).annotation.is_some(),
            // A setter chooses its context when validating the completed list.
            ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => true,
            _ => false,
        }
    }

    fn infer_target(
        &mut self,
        target: &'ast ast::Expr,
        assigned: UnpackAssignedValue<'_, 'db, 'ast>,
    ) -> Option<Type<'db>> {
        match assigned {
            UnpackAssignedValue::Expression(value, tcx) => {
                match target {
                    ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                        // The ordinary write path supplies setter context to a matched
                        // expression and validates the write in the same invocation:
                        //
                        //     class Container:
                        //         def __setitem__(self, key: int, value: list[object]) -> None: ...
                        //
                        //     def assign(container: Container):
                        //         container[0], other = ([1], 0)
                        let shared_source = self.shared_source;
                        let source_contexts = std::mem::take(&mut self.source_contexts);
                        self.infer_member_target(target, value, &|builder, tcx| {
                            infer_source_expression(
                                builder,
                                shared_source,
                                value,
                                tcx,
                                &source_contexts,
                            )
                        });
                        self.source_contexts = source_contexts;
                        // A rejected write may never request the assigned expression.
                        if self
                            .shared_source
                            .and_then(SharedSource::inferred)
                            .is_none()
                        {
                            self.builder
                                .get_or_infer_expression(value, TypeContext::default());
                        }
                    }
                    _ => {
                        let tcx = if target.is_name_expr() {
                            let tcx = self.name_type_context(target);
                            if tcx.annotation.is_some() {
                                // A TypedDict literal reports invalid fields during contextual
                                // inference, so binding validation must not report them again.
                                self.contextual_expressions.insert(value.into());
                            }
                            tcx
                        } else {
                            // When source expressions cannot be paired with individual targets,
                            // use the context selected for the whole source.
                            tcx
                        };
                        infer_source_expression(
                            self.builder,
                            self.shared_source,
                            value,
                            tcx,
                            &self.source_contexts,
                        );
                    }
                }
                // A matched TypedDict literal reports its invalid field during source
                // inference. Validation must not report the same value again as an
                // incompatible assignment to the target.
                self.contextual_expressions.extend(
                    self.source_contexts
                        .keys()
                        .filter(|key| self.builder.expressions.contains_key(key))
                        .copied(),
                );
                None
            }
            UnpackAssignedValue::Captured(source) => {
                let ty = self.infer_captured_list(target, source);
                if let UnpackCaptured::Expressions(elements) = source {
                    // Even an unwritable target cannot prevent the source values from
                    // being inferred; the enclosing expression still needs their types.
                    if ty.is_none()
                        && let Some(source) = self.shared_source
                        && elements.iter().any(Option::is_some)
                    {
                        source.get(self.builder.db());
                    }
                    if self.shared_source.is_none() {
                        for element in elements.iter().flatten() {
                            self.builder
                                .get_or_infer_expression(element, TypeContext::default());
                        }
                    }
                }
                ty
            }
            UnpackAssignedValue::Value(element) => {
                if let ast::Expr::Name(name) = target
                    && let Some(definition) = self.builder.index.try_definition(name)
                {
                    let add = self
                        .name_bindings
                        .remove(&target.into())
                        .unwrap_or_else(|| self.builder.add_binding(target.into(), definition));
                    self.observe_context(target, add.type_context());
                    let (_, bound_ty) = add.insert_with_context(
                        self.builder,
                        element.ty,
                        Some(&self.contextual_expressions),
                    );
                    self.binding_targets.push((definition, bound_ty));
                }
                if matches!(target, ast::Expr::Attribute(_) | ast::Expr::Subscript(_))
                    && !self.validated_targets.contains(&target.into())
                {
                    // A matched literal that did not receive context may contain an
                    // invalid TypedDict field. Pass the whole source expression to
                    // validation so it does not suppress that diagnostic:
                    //
                    //     from typing import TypedDict
                    //
                    //     class Payload(TypedDict):
                    //         value: int
                    //
                    //     def assign(items: list[Payload], values: tuple[int]):
                    //         items[0], other = ({"value": "wrong"}, *values)
                    self.infer_member_target(target, self.value, &|_, _| element.ty);
                }
                None
            }
        }
    }
}
