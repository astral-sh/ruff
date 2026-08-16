//! Implicit instance and class attributes inferred from method assignments.

use super::{MethodDecorator, static_literal::StaticClassLiteral};
use crate::{
    Db, ProgramEnvironment, TypeQualifiers,
    place::{Place, Provenance},
    reachability::binding_reachability,
    types::{
        ClassBase, ClassLiteral, Type, TypeContext, UnionBuilder,
        function::{FunctionDecorators, is_implicit_classmethod, is_implicit_staticmethod},
        infer::{function_known_decorator_flags, infer_unpack_types},
        infer_expression_type, inferred_declaration,
        member::{Member, class_member},
    },
};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ty_python_core::{
    attribute_scopes,
    definition::{Definition, DefinitionKind, DefinitionState, TargetKind},
    place_table,
    scope::ScopeId,
    semantic_index, use_def_map,
};

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    /// Tries to find declarations/bindings of an attribute named `name` that are only
    /// "implicitly" defined (`self.x = …`, `cls.x = …`) in a method of this class.
    /// The `target_method_decorator` parameter is used to skip methods that do not have the
    /// expected decorator.
    pub(super) fn implicit_attribute(
        self,
        db: &'db dyn Db,
        name: &str,
        target_method_decorator: MethodDecorator,
    ) -> Member<'db> {
        self.implicit_attribute_bindings(db, name, target_method_decorator)
            .member
    }

    /// Separate assignments that establish an attribute from assignments that must first read it.
    ///
    /// ```python
    /// class Counter:
    ///     def increment(self):
    ///         self.value += 1
    /// ```
    ///
    /// Here, `value` remains undefined until MRO lookup finds an independent class or instance
    /// attribute. The same rule applies to `cls.value` in a classmethod.
    pub(super) fn implicit_attribute_bindings(
        self,
        db: &'db dyn Db,
        name: &str,
        target_method_decorator: MethodDecorator,
    ) -> ImplicitAttribute<'db> {
        let class_body_scope = self.body_scope(db);
        // Collect names in a tracked query so unrelated edits can preserve dependent member
        // lookups, and avoid retaining query entries for names that no method can define.
        let names = implicit_attribute_names(db, class_body_scope);
        let Ok(name_index) = names.binary_search_by(|candidate| candidate.as_str().cmp(name))
        else {
            return ImplicitAttribute {
                member: Member::unbound(),
                augmented_bindings: None,
            };
        };

        let attribute = ImplicitAttributeName::new(
            db,
            class_body_scope,
            &names[name_index],
            target_method_decorator,
        );

        let facts = implicit_attribute_facts(db, attribute);
        if facts.declarations.is_empty()
            && let Some(incoming) = self.independent_attribute_value(db, attribute)
        {
            let env = ProgramEnvironment::from_scope(class_body_scope);
            let mut incoming_values = UnionBuilder::new(db, &env).add(incoming);
            for &definition in &facts.bindings {
                if let Some(ty) = independent_attribute_assignment_type(db, attribute, definition) {
                    incoming_values.add_in_place(ty);
                }
            }

            Self::implicit_attribute_anchored(db, attribute, incoming_values.build())
        } else {
            Self::implicit_attribute_inner(db, attribute)
        }
    }

    /// Find an attribute value independently established by a proper superclass.
    ///
    /// Inspecting class namespaces never invokes their implicit classmethod lookup. Instance
    /// attributes from superclasses are resolved recursively, always moving toward a strictly
    /// earlier class in the inheritance hierarchy. A default defined by the current class is not
    /// an independent root: using it for cycle recovery could hide other assignments in that class.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, _, _| None,
        heap_size=ruff_memory_usage::heap_size,
    )]
    fn independent_attribute_value(
        self,
        db: &'db dyn Db,
        attribute: ImplicitAttributeName<'db>,
    ) -> Option<Type<'db>> {
        let name = attribute.name(db).as_str();
        let target_method_decorator = attribute.target_method_decorator(db);
        let env = ProgramEnvironment::from_scope(self.body_scope(db));

        for superclass in self.iter_mro(db, None).skip(1) {
            let ClassBase::Class(superclass) = superclass else {
                continue;
            };
            let (literal, specialization) = superclass.class_literal_and_specialization(db);
            let ClassLiteral::Static(literal) = literal else {
                continue;
            };

            let member = class_member(db, literal.body_scope(db), name)
                .map_type(|ty| ty.apply_optional_specialization(db, specialization));
            if member.inner.place.is_definitely_bound()
                && let Ok(ty) = member.inner.into_lookup_result(db, &env)
                && let ty = ty.inner_type()
                && ty.is_definitely_non_data_descriptor(db, &env)
            {
                let receiver_class = self.identity_specialization(db);
                let instance = (target_method_decorator == MethodDecorator::None)
                    .then(|| Type::instance(db, &env, receiver_class));
                return Some(
                    ty.try_call_dunder_get(db, &env, instance, Type::from(receiver_class))
                        .unwrap_or_else(|error| Some(error.fallback()))
                        .map_or(ty, |result| result.return_type),
                );
            }

            let implicit = literal.implicit_attribute_bindings(db, name, target_method_decorator);
            if implicit.member.inner.place.is_definitely_bound()
                && let Some(ty) = implicit.member.ignore_possibly_undefined()
                && !ty.is_divergent()
            {
                return Some(ty.apply_optional_specialization(db, specialization));
            }
        }

        None
    }

    #[salsa::tracked(
        returns(copy),
        cycle_result=|_, _, _, incoming| ImplicitAttribute {
            member: Member {
                inner: Place::bound(incoming)
                    .with_qualifiers(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE),
            },
            augmented_bindings: None,
        },
        heap_size=ruff_memory_usage::heap_size,
    )]
    fn implicit_attribute_anchored(
        db: &'db dyn Db,
        attribute: ImplicitAttributeName<'db>,
        incoming: Type<'db>,
    ) -> ImplicitAttribute<'db> {
        // Independent inherited and local values identify this query and provide an immediate
        // cycle result without discarding assignments that widen the attribute's inferred type.
        let _ = incoming;
        Self::implicit_attribute_impl(db, attribute)
    }

    #[salsa::tracked(
        returns(copy),
        cycle_fn=implicit_attribute_cycle_recover,
        cycle_initial=|_, id, _| ImplicitAttribute {
            member: Member {
                inner: Place::bound(Type::divergent(id)).into(),
            },
            augmented_bindings: None,
        },
        heap_size=ruff_memory_usage::heap_size,
    )]
    fn implicit_attribute_inner(
        db: &'db dyn Db,
        attribute: ImplicitAttributeName<'db>,
    ) -> ImplicitAttribute<'db> {
        Self::implicit_attribute_impl(db, attribute)
    }

    fn implicit_attribute_impl(
        db: &'db dyn Db,
        attribute: ImplicitAttributeName<'db>,
    ) -> ImplicitAttribute<'db> {
        let class_body_scope = attribute.class_body_scope(db);
        let program_file = class_body_scope.program_file(db);
        let python_file = program_file.python_file(db);
        let env = &ProgramEnvironment::from_file(program_file);
        let facts = implicit_attribute_facts(db, attribute);

        // If we do not see any declarations of an attribute, neither in the class body nor in
        // any method, we build a union of the raw types inferred from all bindings of that
        // attribute, then apply public-type promotion to the final union.
        let mut union_of_inferred_types = UnionBuilder::new(db, env);
        let mut qualifiers = TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE;

        let mut is_attribute_bound = false;
        let mut augmented_bindings = Vec::new();
        let mut provenance = Provenance::Unknown;

        let module = parsed_module(db, python_file).load(db);
        let index = semantic_index(db, program_file);

        for &declaration in &facts.declarations {
            let DefinitionKind::AnnotatedAssignment(assignment) = declaration.kind(db) else {
                continue;
            };

            // We found an annotated assignment of one of the following forms (using 'self' in these
            // examples, but we support arbitrary names for the first parameters of methods):
            //
            //     self.name: <annotation>
            //     self.name: <annotation> = …

            let Some(annotation) = inferred_declaration(db, declaration).declared() else {
                continue;
            };
            let annotation = Place::declared(annotation.inner)
                .with_definition(declaration)
                .with_qualifiers(
                    annotation.qualifiers | TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE,
                );

            if let Some(all_qualifiers) = annotation.is_bare_final() {
                if let Some(value) = assignment.value(&module) {
                    // If we see an annotated assignment with a bare `Final` as in
                    // `self.SOME_CONSTANT: Final = 1`, infer the type from the value
                    // on the right-hand side.

                    let inferred_ty =
                        infer_expression_type(db, index.expression(value), TypeContext::default());
                    return ImplicitAttribute {
                        member: Member {
                            inner: Place::bound(inferred_ty)
                                .with_definition(declaration)
                                .with_qualifiers(all_qualifiers),
                        },
                        augmented_bindings: None,
                    };
                }

                // If there is no right-hand side, just record that we saw a `Final` qualifier
                qualifiers |= all_qualifiers;
                continue;
            }

            return ImplicitAttribute {
                member: Member { inner: annotation },
                augmented_bindings: None,
            };
        }

        for &binding in &facts.bindings {
            if matches!(binding.kind(db), DefinitionKind::AugmentedAssignment(_)) {
                augmented_bindings.push(binding);
                continue;
            }

            is_attribute_bound = true;
            if let Some(inferred_ty) = implicit_attribute_binding_type(db, binding) {
                provenance = provenance.or(Provenance::SingleDefinition(binding));
                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
            }
        }

        let member = if is_attribute_bound {
            Member {
                inner: Place::bound(
                    union_of_inferred_types
                        .build()
                        .promote(db, env)
                        .promote_singletons(db, env),
                )
                .with_provenance(provenance)
                .with_qualifiers(qualifiers),
            }
        } else {
            Member::unbound()
        };

        ImplicitAttribute {
            member,
            augmented_bindings: (!augmented_bindings.is_empty())
                .then(|| AugmentedBindings::new(db, augmented_bindings.into_boxed_slice())),
        }
    }
}

/// Attributes assigned by instance methods or classmethods on a single class.
///
/// Ordinary assignments such as `self.value = 1` or `cls.value = 1` establish an attribute
/// directly. Augmented assignments first require an existing instance or class attribute to supply
/// the value they read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ImplicitAttribute<'db> {
    /// The attribute established by assignments that do not depend on an existing value.
    pub(super) member: Member<'db>,
    /// Augmented assignments that require an existing instance or class attribute.
    pub(super) augmented_bindings: Option<AugmentedBindings<'db>>,
}

/// Augmented assignments deferred until MRO lookup finds the attribute they read.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(super) struct AugmentedBindings<'db> {
    #[returns(deref)]
    pub(super) definitions: Box<[Definition<'db>]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for AugmentedBindings<'_> {}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct ImplicitAttributeName<'db> {
    #[returns(copy)]
    class_body_scope: ScopeId<'db>,
    #[returns(ref)]
    name: Name,
    #[returns(copy)]
    target_method_decorator: MethodDecorator,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ImplicitAttributeName<'_> {}

/// Definitions from methods whose receivers can establish this attribute.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct ImplicitAttributeFacts<'db> {
    declarations: Box<[Definition<'db>]>,
    bindings: Box<[Definition<'db>]>,
}

/// Collect declarations and reachable bindings once for ordinary inference and cycle recovery.
///
/// Keeping this shared scan owner-local avoids dependencies on another file's syntax tree and
/// ensures that both inference paths apply the same decorator and method-reachability rules.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn implicit_attribute_facts<'db>(
    db: &'db dyn Db,
    attribute: ImplicitAttributeName<'db>,
) -> ImplicitAttributeFacts<'db> {
    let class_body_scope = attribute.class_body_scope(db);
    let program_file = class_body_scope.program_file(db);
    let index = semantic_index(db, program_file);
    let class_map = use_def_map(db, class_body_scope);
    let class_table = place_table(db, class_body_scope);
    let mut declarations = Vec::new();
    let mut bindings = Vec::new();

    for method_scope_id in attribute_scopes(db, class_body_scope) {
        let method_table = index.place_table(method_scope_id);
        let Some(member) =
            method_table.member_id_by_instance_attribute_name(attribute.name(db).as_str())
        else {
            continue;
        };
        let mut method_scope = index.scope(method_scope_id);
        while method_scope.is_eager()
            && let Some(parent) = method_scope.parent()
        {
            method_scope = index.scope(parent);
        }
        let Some(method_def) = method_scope.node().as_function() else {
            continue;
        };
        let method = index.expect_single_definition(method_def);
        let Some(method_name) = method.name(db) else {
            continue;
        };
        let decorators = function_known_decorator_flags(db, method);
        let is_classmethod = decorators.contains(FunctionDecorators::CLASSMETHOD)
            || is_implicit_classmethod(&method_name);
        let is_staticmethod = decorators.contains(FunctionDecorators::STATICMETHOD)
            || is_implicit_staticmethod(&method_name);
        let is_valid_scope = match attribute.target_method_decorator(db) {
            MethodDecorator::None => !is_classmethod && !is_staticmethod,
            MethodDecorator::ClassMethod => is_classmethod,
            MethodDecorator::StaticMethod => is_staticmethod,
        };
        if !is_valid_scope {
            continue;
        }

        let method_map = index.use_def_map(method_scope_id);
        declarations.extend(method_map.reachable_member_declarations(member).filter_map(
            |declaration| {
                let DefinitionState::Defined(definition) = declaration.declaration else {
                    return None;
                };
                matches!(definition.kind(db), DefinitionKind::AnnotatedAssignment(_))
                    .then_some(definition)
            },
        ));

        let Some(method_place) = class_table.symbol_id(&method_name) else {
            continue;
        };
        if !class_map
            .reachable_symbol_bindings(method_place)
            .any(|binding| {
                binding
                    .binding
                    .is_defined_and(|definition| definition == method)
                    && !binding_reachability(db, class_map, &binding).is_always_false()
            })
        {
            continue;
        }

        bindings.extend(
            method_map
                .reachable_member_bindings(member)
                .filter_map(|binding| match binding.binding {
                    DefinitionState::Defined(definition) => Some(definition),
                    DefinitionState::Undefined | DefinitionState::Deleted => None,
                }),
        );
    }

    ImplicitAttributeFacts {
        declarations: declarations.into_boxed_slice(),
        bindings: bindings.into_boxed_slice(),
    }
}

/// Infer one assignment without allowing a recursive read to establish its own value.
///
/// A cycle involving this exact definition contributes no independent value. Other assignments
/// remain available, so an inherited default cannot hide a local assignment such as `None`.
#[salsa::tracked(
    returns(copy),
    cycle_result=|_, _, _, _| None,
    heap_size=ruff_memory_usage::heap_size,
)]
fn independent_attribute_assignment_type<'db>(
    db: &'db dyn Db,
    attribute: ImplicitAttributeName<'db>,
    definition: Definition<'db>,
) -> Option<Type<'db>> {
    let _ = attribute;
    implicit_attribute_binding_type(db, definition)
}

/// Infer the value written by an attribute definition, including unpacked and iteration targets.
///
/// Ordinary inference and inherited cycle recovery share these projections so that independently
/// written values cannot disappear merely because they were introduced by a `for` or `with`.
fn implicit_attribute_binding_type<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<Type<'db>> {
    let program_file = definition.program_file(db);
    let module = parsed_module(db, program_file.python_file(db)).load(db);
    let index = semantic_index(db, program_file);
    let env = ProgramEnvironment::from_file(program_file);

    match definition.kind(db) {
        DefinitionKind::AnnotatedAssignment(_) => {
            // Annotated assignments are handled before inferring ordinary attribute bindings.
            None
        }
        DefinitionKind::Assignment(assignment) => match assignment.unpack() {
            Some(unpack) => {
                // (..., self.name, ...) = <value>
                let unpacked = infer_unpack_types(db, unpack);
                Some(unpacked.expression_type(assignment.target(&module)))
            }
            None => {
                // self.name = <value>
                Some(infer_expression_type(
                    db,
                    index.expression(assignment.value(&module)),
                    TypeContext::default(),
                ))
            }
        },
        DefinitionKind::For(for_stmt) => match for_stmt.target_kind() {
            TargetKind::Sequence(_, unpack) => {
                // for ..., self.name, ... in <iterable>:
                let unpacked = infer_unpack_types(db, unpack);
                Some(unpacked.expression_type(for_stmt.target(&module)))
            }
            TargetKind::Single => {
                // for self.name in <iterable>:
                let iterable_ty = infer_expression_type(
                    db,
                    index.expression(for_stmt.iterable(&module)),
                    TypeContext::default(),
                );
                // TODO: Potential diagnostics resulting from the iterable are not reported.
                Some(
                    iterable_ty
                        .iterate(db, &env)
                        .homogeneous_element_type(db, &env),
                )
            }
        },
        DefinitionKind::WithItem(with_item) => match with_item.target_kind() {
            TargetKind::Sequence(_, unpack) => {
                // with <context_manager> as ..., self.name, ...:
                let unpacked = infer_unpack_types(db, unpack);
                Some(unpacked.expression_type(with_item.target(&module)))
            }
            TargetKind::Single => {
                // with <context_manager> as self.name:
                let context_ty = infer_expression_type(
                    db,
                    index.expression(with_item.context_expr(&module)),
                    TypeContext::default(),
                );
                Some(if with_item.is_async() {
                    context_ty.aenter(db, &env)
                } else {
                    context_ty.enter(db, &env)
                })
            }
        },
        DefinitionKind::Comprehension(comprehension) => match comprehension.target_kind() {
            TargetKind::Sequence(_, unpack) => {
                // [... for ..., self.name, ... in <iterable>]
                let unpacked = infer_unpack_types(db, unpack);
                Some(unpacked.expression_type(comprehension.target(&module)))
            }
            TargetKind::Single => {
                // [... for self.name in <iterable>]
                let iterable_ty = infer_expression_type(
                    db,
                    index.expression(comprehension.iterable(&module)),
                    TypeContext::default(),
                );
                // TODO: Potential diagnostics resulting from the iterable are not reported.
                Some(
                    iterable_ty
                        .iterate(db, &env)
                        .homogeneous_element_type(db, &env),
                )
            }
        },
        // Named expressions cannot target attributes, and other definitions do not write one.
        _ => None,
    }
}

#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub(super) fn implicit_attribute_names<'db>(
    db: &'db dyn Db,
    class_body_scope: ScopeId<'db>,
) -> Box<[Name]> {
    let index = semantic_index(db, class_body_scope.program_file(db));
    let mut names = Vec::new();

    for function_scope_id in attribute_scopes(db, class_body_scope) {
        let place_table = index.place_table(function_scope_id);
        let use_def = index.use_def_map(function_scope_id);
        names.extend(place_table.members().filter_map(|member| {
            let name = member.as_instance_attribute()?;
            let member_id = place_table.member_id_by_instance_attribute_name(name)?;
            let has_binding = use_def
                .reachable_member_bindings(member_id)
                .any(|binding| matches!(binding.binding, DefinitionState::Defined(_)));
            (has_binding
                || use_def
                    .reachable_member_declarations(member_id)
                    .any(|declaration| {
                        matches!(declaration.declaration, DefinitionState::Defined(_))
                    }))
            .then(|| Name::new(name))
        }));
    }

    names.sort_unstable();
    names.dedup();
    names.into_boxed_slice()
}

fn implicit_attribute_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &ImplicitAttribute<'db>,
    attribute_member: ImplicitAttribute<'db>,
    attribute: ImplicitAttributeName<'db>,
) -> ImplicitAttribute<'db> {
    let env = ProgramEnvironment::from_scope(attribute.class_body_scope(db));
    let inner =
        attribute_member
            .member
            .inner
            .cycle_normalized(db, &env, previous.member.inner, cycle);
    ImplicitAttribute {
        member: Member { inner },
        ..attribute_member
    }
}
