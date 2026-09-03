//! Infer assignment values with type context from their unpacking targets.
//!
//! Target declarations guide inference of nested source expressions and the fresh lists created
//! for starred targets. The results retain individual expression types and contextual capture
//! types for [`Unpacker`], which assigns types to each target and reports unpacking errors.

use itertools::Either;
use ruff_python_ast as ast;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::ExpressionNodeKey;
use ty_python_core::unpack::Unpack;

use super::{AddBinding, CollectionElement, TypeInferenceBuilder};
use crate::place::{DefinedPlace, Place, TypeOrigin};
use crate::types::attribute_write::{AssignmentAttributeMembers, assignment_attribute_members};
use crate::types::tuple::{TupleElement, TupleSpec, TupleSpecBuilder, TupleType};
use crate::types::unpacker::{
    UnpackCaptureContext, UnpackElement, UnpackResult, Unpacker, sequence_elts,
    unpack_literal_values,
};
use crate::types::{KnownClass, Type, TypeContext};

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer and unpack an assignment's right-hand side with context from its targets.
    ///
    /// Return `None` when no target supplies context, so the caller can reuse the cached
    /// ordinary expression inference without storing another copy of its results.
    ///
    /// A starred target receives a new list that has no corresponding list-literal expression on the
    /// right-hand side:
    ///
    /// ```python
    /// rest: list[object]
    /// first, *rest = (0, 1, 2)
    /// ```
    ///
    /// The list assigned to `rest` contains `1` and `2`, but there is no `[1, 2]` expression
    /// whose type we can record. For captures inferred from literal elements, this method passes
    /// their contextual list types to [`Unpacker`] in the `starred_types` field of
    /// [`UnpackCaptureContext`]. That field is a map keyed by the expressions inside starred
    /// targets; here, it contains `list[object]` under the key for `rest`.
    ///
    /// For other captures, `Unpacker` matches the source elements before calling the context's
    /// inference callback, preserving literal positions and union arms. The returned
    /// [`UnpackResult`] contains the assigned target types and the source's expression types.
    pub(in crate::types::infer) fn finish_unpack(
        mut self,
        unpack: Unpack<'db>,
    ) -> Option<UnpackResult<'db>> {
        let db = self.db();
        let env = self.program_environment();
        let module = self.module();
        let target = unpack.target(db, module);
        let value = unpack.value(db);
        let value_expr = value.expression().node_ref(db).node(module);
        if self.unpack_target_type_context(target).annotation.is_none() {
            self.context.defuse();
            return None;
        }

        // Calls to dataclass field specifiers carry metadata such as `init=False`, which
        // controls the generated constructor. That metadata also matters when a field
        // specifier is assigned through unpacking:
        //
        //     from dataclasses import dataclass, field
        //
        //     @dataclass
        //     class Example:
        //         value: int
        //         value, other = field(default=1, init=False), 0
        //
        // `Example.__init__` should have no `value` parameter. Without this setup, we infer
        // `field(...)` using its ordinary return annotation and retain only the default's
        // type. We lose `init=False` and incorrectly add `value: int = 1` to the constructor.
        // `infer_region_expression` normally performs this setup, but this entry point
        // calls `infer_unpack_value` directly so it can supply context from each target.
        self.setup_dataclass_field_specifiers();

        let mut starred_types = FxHashMap::default();
        let mut contextual_expressions = FxHashSet::default();
        self.infer_unpack_value(
            target,
            value_expr,
            &mut starred_types,
            &mut contextual_expressions,
        );
        let mut capture_builder = self.speculate_without_diagnostics();
        let value_inference = self.into_expression_inference();
        let mut infer_capture = |target: &ast::Expr, elements: &[UnpackElement<'db, '_>]| {
            capture_builder.infer_unpacked_captured_list(target, elements)
        };
        let mut unpacker = Unpacker::new(
            db,
            env,
            unpack.target_scope(db),
            unpack.program_file(db),
            module,
            Some(UnpackCaptureContext {
                starred_types,
                infer_from_elements: &mut infer_capture,
            }),
        );
        unpacker.unpack(target, value, &value_inference);
        Some(unpacker.finish(Some(value_inference), contextual_expressions))
    }

    /// Infer each literal element with the declaration of the target that receives it.
    ///
    /// For example, the two list literals below receive independent contexts, even though one
    /// is nested inside another tuple:
    ///
    /// ```python
    /// numbers: list[int]
    /// objects: list[object]
    /// numbers, (objects, last) = ([], ([1], 0))
    /// ```
    ///
    /// Record both the complete source expression's type and the individual element types.
    /// The unpacker needs the latter to distinguish positions in a list: for `a, b = [1, "two"]`,
    /// the overall type `list[int | str]` does not say which element is assigned to `a`.
    fn infer_unpack_value(
        &mut self,
        target: &ast::Expr,
        value: &ast::Expr,
        starred_types: &mut FxHashMap<ExpressionNodeKey, Type<'db>>,
        contextual_expressions: &mut FxHashSet<ExpressionNodeKey>,
    ) {
        // We can pass context to individual source expressions when both sides have tuple
        // or list syntax. For `first, *rest = (0, 1, 2)`, the target slice is `[first, *rest]`
        // and the value slice is `[0, 1, 2]`. We check below whether the values can be
        // distributed among the targets. A source such as `make_values()` has no
        // element expressions to pair with targets, so it uses the fallback at the end.
        let target_elts = sequence_elts(target);

        // Retain the tuple or list node so rebuilding the source's type below is exhaustive.
        let source_sequence = match value {
            ast::Expr::Tuple(tuple) => Some((tuple.elts.as_slice(), Either::Left(tuple))),
            ast::Expr::List(list) => Some((list.elts.as_slice(), Either::Right(list))),
            _ => None,
        };

        // A starred source expression prevents us from mapping syntax directly to targets.
        // For `first, *rest = (*items, 0)`, `first` receives either an element of `items` or
        // `0`, depending on whether `items` is empty. Infer the source's type first instead.
        // Length mismatches also fall back to ordinary inference: for `a, b = (1,)` or
        // `first, *rest, last = (1,)`, let the unpacker report the missing values. Parser
        // recovery can leave multiple starred targets, which have no unambiguous mapping.
        if let Some(targets) = target_elts
            && let Some((values, sequence)) = source_sequence
            && values.iter().all(|value| !value.is_starred_expr())
            && targets
                .iter()
                .filter(|target| target.is_starred_expr())
                .nth(1)
                .is_none()
            && let Some(matched) = unpack_literal_values(targets, value)
        {
            for (target, source) in targets.iter().zip(matched.into_all_elements_with_kind()) {
                if let ast::Expr::Starred(starred) = target
                    && let TupleElement::Variable(captured) = source
                {
                    let tcx = self.unpack_target_type_context(&starred.value);

                    if tcx.annotation.is_some() {
                        // Infer the capture with the same context as a list literal:
                        //
                        //     rest: list[int]
                        //     first, *rest, last = (0, 1, 2, 3)
                        //
                        // Here `rest` collects `[1, 2]`, which we infer with `list[int]`
                        // context. An empty capture uses the annotation in the same way:
                        //
                        //     rest: list[int]
                        //     first, *rest, last = (0, 3)
                        //
                        // Infer this capture as an empty list with `list[int]` context.
                        // Captured literals can themselves use that context:
                        //
                        //     rest: list[list[object]]
                        //     first, *rest = (0, [1], [2])
                        //
                        // Infer `[1]` and `[2]` with `list[object]` context, then construct
                        // `list[list[object]]` as for a list literal containing them.
                        let elts: Vec<[Option<&ast::Expr>; 1]> =
                            captured.into_iter().map(|elt| [elt]).collect();
                        if let Some(ty) = self.infer_collection_literal(
                            KnownClass::List,
                            None,
                            &elts,
                            &mut |builder, (_, elt, tcx)| builder.infer_expression(elt, tcx),
                            tcx,
                        ) {
                            starred_types.insert(starred.value.as_ref().into(), ty);
                        }
                    } else {
                        // Without a declaration for `rest`, `first, *rest = [1, "two"]`
                        // needs no contextual capture type. Leave promotion to the
                        // unpacker's existing rules for tuple and list sources.
                        for value in captured.into_iter().flatten() {
                            self.infer_expression(value, TypeContext::default());
                        }
                    }
                } else if let TupleElement::Fixed(Some(value))
                | TupleElement::Prefix(Some(value))
                | TupleElement::Suffix(Some(value)) = source
                {
                    // A target may itself unpack a nested tuple or list:
                    //
                    //     rest: list[int]
                    //     (first, *rest), other = ((0, 1, 2), 3)
                    //
                    // Recurse with `(first, *rest)` and `(0, 1, 2)` so that `rest`'s
                    // annotation supplies context for the captured values `1` and `2`.
                    // Inferring the inner tuple without inspecting its targets would
                    // miss that annotation.
                    self.infer_unpack_value(target, value, starred_types, contextual_expressions);
                }
            }

            // We still need to record the type of the enclosing tuple or list:
            //
            //     items: list[object]
            //     items, other = ([1], 0)
            //
            // We have inferred `[1]` as `list[object]` and recorded the types of `1`
            // and `0`, but the tuple expression `([1], 0)` has no type yet. Construct
            // its type, `tuple[list[object], Literal[0]]`, using those existing results.
            // Calling ordinary expression inference on the tuple would infer `1`
            // again and fail the assertion in `store_expression_type` that prohibits
            // recording an expression's type twice. The callbacks below reuse the
            // recorded child types instead of repeating their inference.
            let ty = match sequence {
                Either::Left(tuple) => self.infer_tuple_expression_with(
                    tuple,
                    TypeContext::default(),
                    &mut |builder, elt, tcx| builder.get_or_infer_expression(elt, tcx),
                ),
                Either::Right(list) => {
                    let elts: Vec<[Option<&ast::Expr>; 1]> =
                        values.iter().map(|elt| [Some(elt)]).collect();
                    self.infer_collection_literal(
                        KnownClass::List,
                        Some(list.into()),
                        &elts,
                        &mut |builder, (_, elt, tcx)| builder.get_or_infer_expression(elt, tcx),
                        TypeContext::default(),
                    )
                    // Custom typesheds may omit `list` or define it without type parameters.
                    .unwrap_or_else(Type::unknown)
                }
            };
            self.store_expression_type(value, ty);
            return;
        }

        // A single name, attribute, or subscript target receives one expression. For example:
        //
        //     items: list[int]
        //     items, other = ([], 0)
        //
        // In the recursive call for the first target, `target` is `items` and `value` is
        // `[]`; infer that list using the `list[int]` annotation.
        //
        // A tuple or list target reaches here when we cannot pair its source expressions
        // with individual targets, as with a generic call that produces the entire tuple:
        //
        //     def pair[T]() -> tuple[list[T], int]:
        //         return [], 0
        //
        //     items: list[int]
        //     items, other = pair()
        //
        // Combine the target annotations into `tuple[list[int], Unknown]` context and infer
        // `pair()` as a whole. That context lets call inference solve `T` as `int`.
        let tcx = self.unpack_target_type_context(target);

        if tcx.annotation.is_some() && target_elts.is_none() {
            // Contextual inference reports the invalid field value in this assignment:
            //
            //     from typing import TypedDict
            //
            //     class Movie(TypedDict):
            //         title: str
            //
            //     movie: Movie
            //     movie, other = ({"title": 1}, 0)
            //
            // Record that the dictionary literal is inferred with `Movie` context so
            // assignment validation can suppress a duplicate error. The marker belongs to
            // the dictionary expression that receives this context, not the enclosing tuple.
            contextual_expressions.insert(value.into());
        }
        self.infer_expression_impl(value, tcx);
    }

    /// Infer the fresh list made by a starred target from its matched elements.
    ///
    /// ```python
    /// def copy_values(source: list[int]):
    ///     objects: list[object]
    ///     (*objects,) = source
    /// ```
    ///
    /// Here `objects` can have type `list[object]` while `source` remains `list[int]`.
    /// Only the outer list is new: collecting values from `list[list[int]]` does not copy
    /// the inner lists, so those elements cannot be widened to `list[object]`.
    fn infer_unpacked_captured_list(
        &mut self,
        target: &ast::Expr,
        elements: &[UnpackElement<'db, '_>],
    ) -> Option<Type<'db>> {
        let tcx = self.unpack_target_type_context(target);
        tcx.annotation?;

        // The source is already inferred, but each element's syntax still determines whether
        // collection inference may promote its tuple size. Keep literal tuple expressions;
        // elements from existing iterables have no expression and retain their tuple shapes.
        let elts: Vec<[Option<CollectionElement<'db, '_>>; 1]> = elements
            .iter()
            .map(|element| {
                [Some(CollectionElement::Inferred {
                    ty: element.ty,
                    expression: element.expression,
                })]
            })
            .collect();

        self.infer_collection_literal(
            KnownClass::List,
            None,
            &elts,
            &mut |_, _| Type::unknown(),
            tcx,
        )
    }

    /// Combine the declarations on an unpacking target into context for its source expression.
    ///
    /// For the call below, `items` contributes `list[int]` and the unannotated `other`
    /// contributes `Unknown`. The resulting `tuple[list[int], Unknown]` context lets call
    /// inference solve `T` as `int`:
    ///
    /// ```python
    /// def pair[T]() -> tuple[list[T], int]:
    ///     return [], 0
    ///
    /// items: list[int]
    /// items, other = pair()
    /// ```
    fn unpack_target_type_context(&self, target: &ast::Expr) -> TypeContext<'db> {
        let db = self.db();
        let env = self.program_environment();

        if let Some(elts) = sequence_elts(target) {
            let mut tuple = TupleSpecBuilder::with_capacity(elts.len());
            let mut has_context = false;
            let mut has_starred = false;

            for elt in elts {
                if let ast::Expr::Starred(starred) = elt {
                    if has_starred {
                        // Multiple starred targets can occur in a recovered AST, but
                        // `*left, *right = source` has no valid combined tuple context.
                        return TypeContext::default();
                    }
                    has_starred = true;
                    let annotation = self.unpack_target_type_context(&starred.value).annotation;
                    has_context |= annotation.is_some();

                    // A starred target contributes context for individual source elements:
                    //
                    //     def unpack(source: tuple[int, *tuple[str, ...]]):
                    //         first: int
                    //         rest: list[str]
                    //         first, *rest = source
                    //
                    // The target annotations combine into `tuple[int, *tuple[str, ...]]`
                    // context for `source`. Extract `str` from `rest`'s `list[str]`
                    // annotation: the source yields individual strings, and unpacking
                    // collects them into a new list.
                    // An absent or non-iterable annotation (such as `object` or `int`) provides
                    // no element-type context, so that part of the tuple context is `Unknown`.
                    let element_type = annotation
                        .and_then(|ty| ty.try_iterate(db, env).ok())
                        .map(|elements| elements.homogeneous_element_type(db, env))
                        .unwrap_or_else(Type::unknown);

                    tuple = tuple.concat(db, env, &TupleSpec::homogeneous(element_type));
                } else {
                    let annotation = self.unpack_target_type_context(elt).annotation;
                    has_context |= annotation.is_some();
                    tuple.push(annotation.unwrap_or_else(Type::unknown));
                }
            }

            // `a, b = source` with no target declarations should retain ordinary
            // unannotated inference, rather than introducing a tuple of unknown types
            // as a new source of context.
            return TypeContext::new(
                has_context.then(|| Type::tuple(TupleType::new(db, env, &tuple.build()))),
            );
        }

        // This pass only needs declarations. Ordinary target inference later checks the
        // actual assignment, so looking up context must not commit bindings or diagnostics.
        let mut lookup = self.speculate_without_diagnostics();

        match target {
            // Use the ordinary binding machinery to find a name's declaration. Reading its
            // inferred value instead could use the very assignment we are trying to infer.
            // The declaration remains available even when the assignment reads its own target:
            //
            //     def update(flag: bool):
            //         rest: list[int] = [1]
            //         while flag:
            //             first, *rest = (rest[0], 1, 2)
            //
            // Looking up `rest`'s declared type gives us `list[int]` without first inferring
            // the right-hand side, which itself needs the type of `rest` to infer `rest[0]`.
            ast::Expr::Name(name) => self
                .index
                .try_definition(name)
                .map(|definition| lookup.add_binding(target.into(), definition).type_context())
                .unwrap_or_default(),

            ast::Expr::Attribute(attribute) => {
                // An attribute's declaration can provide context for its assigned value:
                //
                //     class Holder:
                //         values: list[object]
                //
                //     def assign(holder: Holder):
                //         holder.values, other = ([1], 0)
                //
                // Infer `[1]` with `list[object]` context from `Holder.values`. The same
                // declaration provides context if `values` is listed in `Holder.__slots__`.
                let receiver = lookup
                    .infer_maybe_standalone_expression(&attribute.value, TypeContext::default());

                // A descriptor's read type need not be the type accepted by assignment:
                //
                //     class Holder:
                //         @property
                //         def value(self) -> list[str]:
                //             return []
                //
                //         @value.setter
                //         def value(self, value: list[int]) -> None:
                //             pass
                //
                //     def assign(holder: Holder):
                //         holder.value, other = ([1], 0)
                //
                // The setter accepts `list[int]`, so the getter's `list[str]` return type
                // is not suitable context for `[1]`. Leave context unset for these descriptors;
                // assignment validation checks the setter separately. Slot descriptors use
                // their declared type for both reads and writes, so they can provide context.
                if assignment_attribute_members(db, env, receiver, &attribute.attr)
                    .and_then(AssignmentAttributeMembers::type_member)
                    .and_then(|member| member.place.ignore_possibly_undefined())
                    .is_some_and(|ty| {
                        ty.may_be_data_descriptor(db, env) && !matches!(ty, Type::SlotDescriptor(_))
                    })
                {
                    return TypeContext::default();
                }

                // An inferred attribute must not provide context for its own definition:
                //
                //     class Holder:
                //         def __init__(self) -> None:
                //             self.values, other = ([1], 0)
                //
                // There is no annotation for `values`, so infer `[1]` from its elements.
                // Using the attribute's inferred type as context would depend on the very
                // assignment whose value we are currently inferring.
                TypeContext::new(match receiver.member(db, env, &attribute.attr).place {
                    Place::Defined(DefinedPlace {
                        ty,
                        origin: TypeOrigin::Declared,
                        ..
                    }) => Some(ty),
                    _ => None,
                })
            }

            ast::Expr::Subscript(subscript) => {
                // A list's element type describes both reads and writes through a subscript:
                //
                //     def assign(rows: list[list[object]]):
                //         rows[0], other = ([1], 0)
                //
                // `rows[0]` has declared element type `list[object]`, which supplies context
                // for `[1]`. TypedDict keys similarly have declared value types. Arbitrary
                // `__getitem__` and `__setitem__` methods can disagree, so their read types
                // are not generally valid assignment contexts.
                let receiver = lookup
                    .infer_maybe_standalone_expression(&subscript.value, TypeContext::default());
                if receiver.is_typed_dict() || AddBinding::is_safe_mutable_class(db, env, receiver)
                {
                    TypeContext::new(lookup.fallback_member_declared_type(target.into()))
                } else {
                    TypeContext::default()
                }
            }

            _ => TypeContext::default(),
        }
    }
}
