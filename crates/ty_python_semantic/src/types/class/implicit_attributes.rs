//! Implicit instance and class attributes inferred from method assignments.

use super::{MethodDecorator, static_literal::StaticClassLiteral};
use crate::{
    Db, ProgramEnvironment, TypeQualifiers, attribute_assignments, attribute_declarations,
    place::{Place, Provenance},
    reachability::binding_reachability,
    types::{
        KnownClass, Truthiness, Type, TypeContext, UnionBuilder, definition_expression_type,
        function::{is_implicit_classmethod, is_implicit_staticmethod},
        infer::infer_unpack_types,
        infer_expression_type, inferred_declaration,
        member::Member,
    },
};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ty_python_core::{
    attribute_scopes,
    definition::{Definition, DefinitionKind, DefinitionState, TargetKind},
    place_table,
    scope::{Scope, ScopeId},
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

        Self::implicit_attribute_inner(
            db,
            ImplicitAttributeName::new(
                db,
                class_body_scope,
                &names[name_index],
                target_method_decorator,
            ),
        )
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
        let name = attribute.name(db).as_str();
        let target_method_decorator = attribute.target_method_decorator(db);
        let program_file = class_body_scope.program_file(db);
        let python_file = program_file.python_file(db);
        let env = &ProgramEnvironment::from_file(program_file);

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
            attribute_declarations(db, class_body_scope, name)
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

                        let inferred_ty = infer_expression_type(
                            db,
                            index.expression(value),
                            TypeContext::default(),
                        );
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
        }

        for (attribute_assignments, attribute_binding_scope_id) in
            attribute_assignments(db, class_body_scope, name)
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

                if matches!(binding.kind(db), DefinitionKind::AugmentedAssignment(_)) {
                    augmented_bindings.push(binding);
                    continue;
                }

                if !is_method_reachable.is_always_false() {
                    is_attribute_bound = true;
                }

                let inferred_ty = implicit_attribute_binding_type(db, binding);

                if let Some(inferred_ty) = inferred_ty {
                    provenance = provenance.or(Provenance::SingleDefinition(binding));
                    union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                }
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

/// Infer the value written by an attribute definition, including unpacked and iteration targets.
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
        names.extend(
            index
                .place_table(function_scope_id)
                .members()
                .filter_map(|member| member.as_instance_attribute().map(Name::new)),
        );
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
