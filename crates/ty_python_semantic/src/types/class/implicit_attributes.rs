//! Implicit instance and class attributes inferred from method assignments.

use super::{MethodDecorator, static_literal::StaticClassLiteral};
use crate::{
    Db, ProgramEnvironment, TypeQualifiers, attribute_assignments, attribute_declarations,
    place::{Place, PlaceAndQualifiers, Provenance},
    reachability::binding_reachability,
    types::{
        KnownClass, Type, TypeContext, UnionBuilder, definition_expression_type,
        function::{is_implicit_classmethod, is_implicit_staticmethod},
        infer::infer_unpack_types,
        infer_expression_type, inferred_declaration,
        member::Member,
    },
};
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::name::Name;
use ty_python_core::{
    SemanticIndex, attribute_scopes,
    definition::{
        AnnotatedAssignmentDefinitionKind, Definition, DefinitionKind, DefinitionState, TargetKind,
    },
    place_table,
    scope::{Scope, ScopeId},
    semantic_index, use_def_map,
};

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    /// Find an instance annotation without inferring unannotated attribute assignments.
    pub(super) fn implicit_instance_declaration(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> Option<PlaceAndQualifiers<'db>> {
        let scope = self.body_scope(db);
        let names = implicit_attribute_names(db, scope);
        let name_index = names
            .binary_search_by(|candidate| candidate.as_str().cmp(name))
            .ok()?;
        Self::implicit_instance_declaration_inner(
            db,
            ImplicitAttributeName::new(db, scope, &names[name_index], MethodDecorator::None),
        )
    }

    #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    fn implicit_instance_declaration_inner(
        db: &'db dyn Db,
        attribute: ImplicitAttributeName<'db>,
    ) -> Option<PlaceAndQualifiers<'db>> {
        implicit_attribute_declarations(
            db,
            attribute.class_body_scope(db),
            attribute.name(db),
            MethodDecorator::None,
        )
        .next()
        .map(|(_, _, annotation)| annotation)
    }

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
        // First check declarations
        for (declaration, assignment, annotation) in
            implicit_attribute_declarations(db, class_body_scope, name, target_method_decorator)
        {
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

        for (attribute_assignments, attribute_binding_scope_id) in
            attribute_assignments(db, class_body_scope, name)
        {
            let binding_scope = index.scope(attribute_binding_scope_id);
            if !is_valid_scope(db, index, &module, binding_scope, target_method_decorator)
                || !is_reachable_method(db, index, &module, class_body_scope, binding_scope)
            {
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

                is_attribute_bound = true;

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

/// Find annotations on attributes of the receiver in reachable methods.
///
/// This includes both `self.name: T` and `self.name: T = value`. The receiver can use any name
/// chosen for the method's first parameter.
fn implicit_attribute_declarations<'db, 'name>(
    db: &'db dyn Db,
    class_body_scope: ScopeId<'db>,
    name: &'name str,
    target_method_decorator: MethodDecorator,
) -> impl Iterator<
    Item = (
        Definition<'db>,
        &'db AnnotatedAssignmentDefinitionKind,
        PlaceAndQualifiers<'db>,
    ),
> + use<'db, 'name> {
    let file = class_body_scope.program_file(db);
    let module = parsed_module(db, file.python_file(db)).load(db);
    let index = semantic_index(db, file);

    attribute_declarations(db, class_body_scope, name)
        .filter(move |(_, method_scope_id)| {
            let method_scope = index.scope(*method_scope_id);
            is_valid_scope(db, index, &module, method_scope, target_method_decorator)
                && is_reachable_method(db, index, &module, class_body_scope, method_scope)
        })
        .flat_map(|(declarations, _)| declarations)
        .filter_map(move |declaration| {
            let DefinitionState::Defined(declaration) = declaration.declaration else {
                return None;
            };
            let DefinitionKind::AnnotatedAssignment(assignment) = declaration.kind(db) else {
                return None;
            };
            let annotation = inferred_declaration(db, declaration).declared()?;
            Some((
                declaration,
                assignment,
                Place::declared(annotation.inner)
                    .with_definition(declaration)
                    .with_qualifiers(
                        annotation.qualifiers | TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE,
                    ),
            ))
        })
}

/// An attribute in a method can contribute only if the method can be defined.
fn is_reachable_method<'db>(
    db: &'db dyn Db,
    index: &'db SemanticIndex<'db>,
    module: &ParsedModuleRef,
    class_body_scope: ScopeId<'db>,
    mut scope: &'db Scope,
) -> bool {
    while scope.is_eager()
        && let Some(parent) = scope.parent()
    {
        scope = index.scope(parent);
    }

    let Some(method_def) = scope.node().as_function() else {
        return false;
    };
    let method = index.expect_single_definition(method_def);
    let Some(method_symbol) =
        place_table(db, class_body_scope).symbol_id(&method_def.node(module).name)
    else {
        return false;
    };
    let class_map = use_def_map(db, class_body_scope);
    class_map
        .reachable_symbol_bindings(method_symbol)
        .any(|binding| {
            binding
                .binding
                .is_defined_and(|definition| definition == method)
                && !binding_reachability(db, class_map, &binding).is_always_false()
        })
}

fn is_valid_scope<'db>(
    db: &'db dyn Db,
    index: &SemanticIndex<'db>,
    module: &ParsedModuleRef,
    method_scope: &Scope,
    target_method_decorator: MethodDecorator,
) -> bool {
    let Some(method_def) = method_scope.node().as_function() else {
        return true;
    };

    // Check the decorators directly on the AST node to determine if this method
    // is a classmethod or staticmethod. This is more reliable than checking the
    // final evaluated type, which may be wrapped by other decorators like @cache.
    let function_node = method_def.node(module);
    let definition = index.expect_single_definition(method_def);
    let mut is_classmethod = false;
    let mut is_staticmethod = false;

    for decorator in &function_node.decorator_list {
        let decorator_ty = definition_expression_type(db, definition, &decorator.expression);
        if let Type::ClassLiteral(class) = decorator_ty {
            match class.known(db) {
                Some(KnownClass::Classmethod) => is_classmethod = true,
                Some(KnownClass::Staticmethod) => is_staticmethod = true,
                _ => {}
            }
        }
    }

    // Also check for implicit classmethods/staticmethods based on method name.
    let method_name = function_node.name.as_str();
    is_classmethod |= is_implicit_classmethod(method_name);
    is_staticmethod |= is_implicit_staticmethod(method_name);

    match target_method_decorator {
        MethodDecorator::None => !is_classmethod && !is_staticmethod,
        MethodDecorator::ClassMethod => is_classmethod,
        MethodDecorator::StaticMethod => is_staticmethod,
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
