pub(in crate::types::infer::builder) mod argument_checks;
pub(in crate::types::infer::builder) mod base_checks;
pub(in crate::types::infer::builder) mod dataclass_application;
pub(in crate::types::infer::builder) mod disjoint_decorator;
mod effects;
pub(in crate::types::infer::builder) mod enum_checks;
pub(in crate::types::infer::builder) mod final_values;
pub(in crate::types::infer::builder) mod generic_checks;
pub(in crate::types::infer::builder) mod metaclass_checks;
pub(in crate::types::infer::builder) mod mro_checks;
pub(in crate::types::infer::builder) mod phases;
pub(in crate::types::infer::builder) mod slot_checks;
pub(in crate::types::infer::builder) mod total_ordering;

use crate::Db;
use itertools::Itertools;
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;

use crate::attribute_assignments;
use crate::{
    TypeQualifiers,
    place::{DefinedPlace, Place, TypeOrigin, place_from_declarations},
    types::{
        ClassBase, ClassType, DataclassFlags, StaticClassLiteral, Type,
        abstract_methods::AbstractMethods,
        binding_type,
        class::{CodeGeneratorKind, Field, FieldKind},
        context::InferContext,
        diagnostic::{
            ABSTRACT_METHOD_IN_FINAL_CLASS, CYCLIC_CLASS_DEFINITION, DATACLASS_FIELD_ORDER,
            DUPLICATE_KW_ONLY, report_invalid_attribute_assignment,
            report_invalid_named_tuple_field_qualifier,
        },
        function::KnownFunction,
        infer_definition_types,
        special_form::TypeQualifier,
    },
};
use ty_python_core::{
    SemanticIndex, attribute_scopes, definition::DefinitionKind, scope::ScopeId, semantic_index,
};

/// Rejects slot layouts that fail while Python constructs the runtime class.
///
/// ```python
/// class Example:
///     __slots__ = ("value",)
///     value = 1  # This class binding conflicts with the generated slot descriptor.
/// ```
///
/// Stub declarations do not execute and therefore cannot create runtime class-namespace conflicts.
fn check_class_slots<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    index: &SemanticIndex<'db>,
) {
    match slot_checks::check_class_slots_sync(
        class,
        &slot_checks::OrdinaryClassSlotCheckEffects { context, index },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

/// Iterate over all static class definitions (created using `class` statements) to check that
/// the definition is semantically valid and will not cause an exception to be raised at runtime.
/// This needs to be done after most other types in the scope have been inferred, due to the fact
/// that base classes can be deferred. If it looks like a class definition is invalid in some way,
/// issue a diagnostic.
///
/// Note: Dynamic classes created via `type()` calls are checked separately during type
/// inference of the call expression.
///
/// Among the things we check for in this method are whether Python will be able to determine a
/// consistent "[method resolution order]" and [metaclass] for each class.
///
/// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
/// [metaclass]: https://docs.python.org/3/reference/datamodel.html#metaclasses
pub(crate) fn check_static_class_definitions<'db>(
    context: &InferContext<'db, '_>,
    ty: Type<'db>,
    class_node: &ast::StmtClassDef,
    index: &SemanticIndex<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    match phases::check_static_class_definitions_sync(
        ty,
        class_node,
        &phases::OrdinaryStaticClassDefinitionEffects {
            context,
            index,
            file_expression_type,
        },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

fn check_inheritance_cycle<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
) -> bool {
    let db = context.db();

    // Check that the class does not have a cyclic definition
    if let Some(inheritance_cycle) = class.inheritance_cycle(context.db()) {
        if inheritance_cycle.is_participant()
            && let Some(builder) = context.report_lint(&CYCLIC_CLASS_DEFINITION, class_node)
        {
            builder.into_diagnostic(format_args!(
                "Cyclic definition of `{}` (class cannot inherit from itself)",
                class.name(db)
            ));
        }

        // If a class is cyclically defined, that's a sufficient error to report; the
        // following checks (which are all inheritance-based) aren't even relevant.
        return true;
    }
    false
}

fn check_generic_enum<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
) {
    match enum_checks::check_generic_enum_sync(
        class,
        class_node,
        &enum_checks::OrdinaryGenericEnumEffects { context },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

fn check_named_tuple<'db>(context: &InferContext<'db, '_>, class: StaticClassLiteral<'db>) {
    let db = context.db();

    // `ClassVar` and `Final` fields have to be checked against the class body's annotations
    // rather than against `own_fields`, since `own_fields` drops `ClassVar` declarations and
    // does not retain the `Final` qualifier for the fields that it does keep.
    //
    // A field carrying both qualifiers is reported once per qualifier, since each qualifier
    // independently violates the restriction on `NamedTuple` fields.
    for (field_name, qualifiers, declaration) in class.own_annotated_qualifiers(db) {
        let invalid_qualifiers = [TypeQualifier::ClassVar, TypeQualifier::Final]
            .into_iter()
            .filter(|qualifier| qualifiers.contains(TypeQualifiers::from(*qualifier)));

        for qualifier in invalid_qualifiers {
            report_invalid_named_tuple_field_qualifier(
                context,
                &field_name,
                qualifier,
                declaration,
            );
        }
    }

    // Check that no field without a default value appears after a field with a default value.
    effects::check_named_tuple_fields(context, class);
}

fn check_disjoint_base_decorator<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    class_kind: Option<CodeGeneratorKind<'db>>,
    is_protocol: bool,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    match disjoint_decorator::check_disjoint_base_decorator_sync(
        class,
        class_node,
        class_kind,
        is_protocol,
        &disjoint_decorator::OrdinaryDisjointBaseDecoratorEffects {
            context,
            file_expression_type,
        },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

fn check_dataclass_application<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    is_protocol: bool,
) {
    match dataclass_application::check_dataclass_application_sync(
        class,
        is_protocol,
        &dataclass_application::OrdinaryDataclassApplicationEffects { context },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

fn check_explicit_bases<'node, 'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &'node ast::StmtClassDef,
    index: &SemanticIndex<'db>,
    class_kind: Option<CodeGeneratorKind<'db>>,
    is_protocol: bool,
) -> phases::StaticClassBaseChecks<'node, 'db> {
    match base_checks::check_explicit_bases_sync(
        class,
        class_node,
        class_kind,
        is_protocol,
        base_checks::ExplicitBaseCheckFacts,
        &base_checks::OrdinaryExplicitBaseCheckEffects { context, index },
    ) {
        Ok(bases) => bases,
        Err(never) => match never {},
    }
}

fn check_mro<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    base_checks: &mut phases::StaticClassBaseChecks<'_, 'db>,
) -> bool {
    match mro_checks::check_mro_sync(
        class,
        class_node,
        base_checks,
        &mro_checks::OrdinaryMroCheckEffects { context },
    ) {
        Ok(inconsistent) => inconsistent,
        Err(never) => match never {},
    }
}

fn check_total_ordering<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    match total_ordering::check_total_ordering_sync(
        class,
        class_node,
        &total_ordering::OrdinaryTotalOrderingEffects {
            context,
            file_expression_type,
        },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

fn check_metaclass<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
) {
    match metaclass_checks::check_metaclass_sync(
        class,
        class_node,
        &metaclass_checks::OrdinaryMetaclassCheckEffects { context },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

fn check_arguments<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    class_kind: Option<CodeGeneratorKind<'db>>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    match argument_checks::check_arguments_sync(
        class,
        class_node,
        class_kind,
        argument_checks::ClassArgumentCheckFacts,
        &argument_checks::OrdinaryClassArgumentCheckEffects {
            context,
            file_expression_type,
        },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

fn check_generic_context<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    index: &SemanticIndex<'db>,
) {
    match generic_checks::check_generic_context_sync(
        class,
        class_node,
        generic_checks::ClassGenericCheckFacts,
        &generic_checks::OrdinaryClassGenericCheckEffects { context, index },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

fn check_dataclass_fields<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    field_policy: CodeGeneratorKind<'db>,
    index: &SemanticIndex<'db>,
) {
    let db = context.db();

    // Check that a dataclass does not have more than one `KW_ONLY`
    // and that required fields are defined before default fields.
    let specialization = None;
    let class_init = class.has_dataclass_param(db, field_policy, DataclassFlags::INIT);
    let own_fields = class.own_fields(db, specialization, field_policy);

    let kw_only_sentinel_fields: Vec<_> = own_fields
        .iter()
        .filter_map(|(name, field)| field.is_kw_only_sentinel(db).then_some(name))
        .collect();
    let mut field_order_violations = vec![];
    let mut previous_default_field = None;

    for (name, field) in class.fields(db, specialization, field_policy) {
        // Extract dataclass field properties
        let FieldKind::Dataclass {
            default_ty,
            init,
            kw_only,
            ..
        } = &field.kind
        else {
            continue;
        };

        // Classes or fields with init=False and kw_only fields don't participate in ordering.
        if !class_init || !init || *kw_only == Some(true) {
            continue;
        }

        if default_ty.is_some() {
            previous_default_field = Some((name, field));
        } else if let Some((default_name, default_field)) = previous_default_field {
            field_order_violations.push((default_name, default_field, name, field));
        }
    }

    if kw_only_sentinel_fields.len() > 1 {
        // TODO: The fields should be displayed in a subdiagnostic.
        if let Some(builder) = context.report_lint(&DUPLICATE_KW_ONLY, &class_node.name) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Dataclass has more than one field annotated with `KW_ONLY`"
            ));

            diagnostic.info(format_args!(
                "`KW_ONLY` fields: {}",
                kw_only_sentinel_fields
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .join(", ")
            ));
        }
    }

    if !field_order_violations.is_empty() {
        let body_scope = class.body_scope(db).file_scope_id(db);
        let use_def_map = index.use_def_map(body_scope);
        let place_table = index.place_table(body_scope);

        for (default_name, default_field, name, field) in field_order_violations {
            if !own_fields.contains_key(default_name)
                && !own_fields.contains_key(name)
                && has_inherited_dataclass_field_order_violation(
                    db,
                    class,
                    default_name,
                    default_field,
                    name,
                    field,
                )
            {
                continue;
            }

            let report = |range: TextRange| {
                let Some(builder) = context.report_lint(&DATACLASS_FIELD_ORDER, range) else {
                    return false;
                };
                builder.into_diagnostic(format_args!(
                    "Required field `{name}` cannot be defined after fields with default values",
                ));
                true
            };

            if !own_fields.contains_key(name) {
                report(class_node.name.range());
                continue;
            }

            let Some(symbol_id) = place_table.symbol_id(name.as_str()) else {
                continue;
            };
            for decl_with_constraints in use_def_map.end_of_scope_symbol_declarations(symbol_id) {
                if let Some(definition) = decl_with_constraints.declaration.definition()
                    && let DefinitionKind::AnnotatedAssignment(ann_assign) = definition.kind(db)
                    && report(ann_assign.target(context.module()).range())
                {
                    break;
                }
            }
        }
    }
}

/// Returns whether the same default-before-required field pair already violates an ancestor's
/// generated constructor ordering.
///
/// ```python
/// from dataclasses import dataclass
///
/// @dataclass
/// class Base:
///     optional: int = 1
///     required: int
///
/// @dataclass
/// class Child(Base):
///     pass
/// ```
///
/// `Child` inherits the existing error and should not report it again. Comparing declaration
/// provenance preserves diagnostics when a subclass redeclares either field.
fn has_inherited_dataclass_field_order_violation<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    default_name: &Name,
    default_field: &Field<'db>,
    required_name: &Name,
    required_field: &Field<'db>,
) -> bool {
    class
        .iter_mro(db, None)
        .skip(1)
        .filter_map(ClassBase::into_class)
        .filter_map(|ancestor| ancestor.static_class_literal(db))
        .any(|(ancestor, specialization)| {
            let Some(field_policy @ CodeGeneratorKind::DataclassLike(_)) =
                CodeGeneratorKind::from_class(db, ancestor.into())
            else {
                return false;
            };
            if !ancestor.has_dataclass_param(db, field_policy, DataclassFlags::INIT) {
                return false;
            }

            let fields = ancestor.fields(db, specialization, field_policy);
            let Some((default_index, _, inherited_default_field)) = fields.get_full(default_name)
            else {
                return false;
            };
            let Some((required_index, _, inherited_required_field)) =
                fields.get_full(required_name)
            else {
                return false;
            };

            default_index < required_index
                && inherited_default_field.first_declaration == default_field.first_declaration
                && inherited_required_field.first_declaration == required_field.first_declaration
        })
}

/// Check compatibility between class namespace values and attributes populated by its metaclass.
///
/// A binding in a class body is passed through the namespace used to construct the class object
/// before a metaclass can initialize that same attribute. If the metaclass declares a type for
/// that attribute, an incompatible class-body value violates that contract.
///
/// Independently, an explicit attribute declaration in the class body constrains a value
/// populated by metaclass initialization.
fn check_class_namespace_against_metaclass_members<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    metaclass: Type<'db>,
    index: &SemanticIndex<'db>,
) {
    let db = context.db();
    let env = context.program_environment();

    let Some(metaclass_instance) = metaclass.to_instance_approximation(db, env) else {
        return;
    };

    let scope = class.body_scope(db).file_scope_id(db);
    let table = index.place_table(scope);
    let use_def = index.use_def_map(scope);

    let Some(metaclass) = metaclass.to_class_type(db) else {
        return;
    };

    // Metaclass-populated members are generally sparse, while class namespaces such as enums can
    // be large. Collect possible members first rather than probing the metaclass for every binding.
    let mut metaclass_instance_members = FxHashSet::default();
    let mut metaclass_assigned_members = FxHashSet::default();
    for metaclass in metaclass
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .filter_map(|class| class.static_class_literal(db).map(|(literal, _)| literal))
    {
        let body_scope = metaclass.body_scope(db);
        let metaclass_index = semantic_index(db, body_scope.program_file(db));
        let body_scope_id = body_scope.file_scope_id(db);
        let metaclass_table = metaclass_index.place_table(body_scope_id);
        let metaclass_use_def = metaclass_index.use_def_map(body_scope_id);

        for (symbol_id, _) in metaclass_use_def.all_end_of_scope_symbol_declarations() {
            metaclass_instance_members.insert(metaclass_table.symbol(symbol_id).name().clone());
        }

        for function_scope in attribute_scopes(db, body_scope) {
            for member in metaclass_index.place_table(function_scope).members() {
                if let Some(name) = member.as_instance_attribute() {
                    // A method-scope member may only be declared, as in `cls.attr: int`.
                    // Only an assignment such as `cls.attr: int = 1` writes a value onto the
                    // newly created class object, potentially overwriting a class-body value.
                    let is_assigned =
                        attribute_assignments(db, body_scope, name).any(|(bindings, _)| {
                            bindings
                                .into_iter()
                                .any(|binding| binding.binding.definition().is_some())
                        });
                    let name = Name::new(name);
                    if is_assigned {
                        metaclass_assigned_members.insert(name.clone());
                    }
                    metaclass_instance_members.insert(name);
                }
            }
        }
    }

    #[expect(
        clippy::iter_over_hash_type,
        reason = "each metaclass member is checked independently"
    )]
    for name in metaclass_instance_members {
        let Some(symbol_id) = table.symbol_id(name.as_str()) else {
            continue;
        };
        let Place::Defined(DefinedPlace {
            ty: metaclass_member_ty,
            origin,
            ..
        }) = metaclass_instance
            .instance_member(db, env, name.as_str())
            .place
        else {
            continue;
        };

        if origin == TypeOrigin::Declared {
            let mut reported_incompatible_binding = false;
            for binding in use_def.end_of_scope_symbol_bindings(symbol_id) {
                let Some(definition) = binding.binding.definition() else {
                    continue;
                };
                let definition_kind = definition.kind(db);
                if !definition_kind.is_user_visible() {
                    continue;
                }

                let assigned_ty = binding_type(db, definition);
                if !assigned_ty.is_assignable_to(db, env, metaclass_member_ty) {
                    reported_incompatible_binding = true;
                    report_invalid_attribute_assignment(
                        context,
                        definition_kind.target_range(context.module()),
                        metaclass_member_ty,
                        assigned_ty,
                        name.as_str(),
                    );
                }
            }

            if reported_incompatible_binding {
                continue;
            }
        }

        // A declaration on the metaclass constrains class-object access, but does not itself
        // populate a replacement value into the constructed class's namespace.
        if !metaclass_assigned_members.contains(name.as_str()) {
            continue;
        }

        let result =
            place_from_declarations(db, env, use_def.end_of_scope_symbol_declarations(symbol_id));
        let Some(definition) = result.first_declaration else {
            continue;
        };
        let Place::Defined(DefinedPlace {
            ty: class_declared_ty,
            ..
        }) = result.ignore_conflicting_declarations().place
        else {
            continue;
        };
        let definition_kind = definition.kind(db);
        // A metaclass may intentionally replace an ordinary class namespace value during class
        // creation. Only an explicit attribute annotation constrains the replacement value.
        if !matches!(definition_kind, DefinitionKind::AnnotatedAssignment(_)) {
            continue;
        }
        if !metaclass_member_ty.is_assignable_to(db, env, class_declared_ty) {
            report_invalid_attribute_assignment(
                context,
                definition_kind.target_range(context.module()),
                class_declared_ty,
                metaclass_member_ty,
                name.as_str(),
            );
        }
    }
}

fn ordered_dataclass_base_class<'db>(
    db: &'db dyn Db,
    base_class: ClassType<'db>,
) -> Option<ClassType<'db>> {
    for ancestor in base_class.iter_mro(db).filter_map(ClassBase::into_class) {
        let Some((ancestor_literal, _)) = ancestor.static_class_literal(db) else {
            continue;
        };

        if ancestor_literal.is_ordered_dataclass(db) {
            return Some(ancestor);
        }

        if ancestor_literal.has_own_comparison_methods(db) {
            return None;
        }
    }

    None
}

/// Check that a `@final` class does not have unimplemented abstract methods.
///
/// A final class cannot be subclassed, so if it inherits abstract methods without
/// implementing them, those methods can never be implemented, making the class
/// effectively broken.
fn check_final_class_abstract_methods<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
) {
    let db = context.db();
    let env = context.program_environment();

    let class_type = class.identity_specialization(db);
    let abstract_methods = AbstractMethods::of_class(db, class_type);

    // If there are no abstract methods, we're done.
    let Some(first_method_name) = abstract_methods.first_name() else {
        return;
    };

    let Some(builder) = context.report_lint(&ABSTRACT_METHOD_IN_FINAL_CLASS, &class_node.name)
    else {
        return;
    };

    let class_name = class.name(db);

    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Final class `{class_name}` has unimplemented abstract methods",
    ));

    let definition_types = infer_definition_types(db, class.definition(db));

    if let Some(class_node) = class.body_scope(db).node(db).as_class()
        && let Some(decorator) = class_node
            .node(context.module())
            .decorator_list
            .iter()
            .find(|decorator| {
                definition_types
                    .expression_type(&decorator.expression)
                    .as_function_literal()
                    .is_some_and(|function| function.is_known(db, KnownFunction::Final))
            })
    {
        diagnostic.annotate(context.secondary(decorator));
    }

    abstract_methods.annotate_diagnostic(db, env, &mut diagnostic);
    let num_abstract_methods = abstract_methods.len();
    if num_abstract_methods == 1 {
        diagnostic.set_concise_message(format_args!(
            "Final class `{class_name}` has unimplemented abstract method `{first_method_name}`",
        ));
    } else {
        let formatted_methods = abstract_methods.formatted_names(db);
        if formatted_methods.truncation_occurred {
            diagnostic.set_concise_message(format_args!(
                "Final class `{class_name}` has {num_abstract_methods} unimplemented \
                    abstract methods, including {formatted_methods}",
            ));
        } else {
            diagnostic.set_concise_message(format_args!(
                "Final class `{class_name}` has unimplemented abstract methods {formatted_methods}",
            ));
        }
    }
}

/// Check for `Final`-qualified declarations in a class body scope that are never
/// assigned a value.
fn check_class_final_without_value<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    index: &SemanticIndex<'db>,
) {
    match final_values::check_class_final_without_value_sync(
        class,
        final_values::ClassFinalValueFacts,
        &final_values::OrdinaryClassFinalValueEffects { context, index },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

/// Returns `true` if `name` has any attribute assignment (`self.<name> = ...`) in an
/// `__init__` method of the class whose body scope is `class_body_scope`.
fn has_binding_in_init<'db>(
    context: &InferContext<'db, '_>,
    class_body_scope: ScopeId<'db>,
    index: &SemanticIndex<'db>,
    name: &str,
) -> bool {
    let db = context.db();
    attribute_assignments(db, class_body_scope, name).any(|(bindings, scope_id)| {
        let is_init = index
            .scope(scope_id)
            .node()
            .as_function()
            .is_some_and(|f| f.node(context.module()).name.id == "__init__");
        is_init
            && bindings
                .into_iter()
                .any(|b| b.binding.definition().is_some())
    })
}
