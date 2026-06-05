use crate::{
    diagnostic::format_enumeration,
    types::{
        KnownClass, KnownInstanceType, ParamSpecAttrKind, Signature, SpecialFormType, Type,
        TypeAliasType, TypeVarBoundOrConstraints, TypeVarKind, UnionType,
        context::InferContext,
        definition_expression_type,
        diagnostic::{
            INVALID_LEGACY_POSITIONAL_PARAMETER, INVALID_METHOD_RECEIVER,
            INVALID_TYPE_VARIABLE_DEFAULT,
        },
        function::{FunctionDecorators, OverloadLiteral},
        infer::original_class_type,
        infer_definition_types,
        signatures::ReturnCallableTypeVarScope,
        tuple::Tuple,
        typevar::TypeVarInstance,
        visitor::{any_over_type, find_over_type},
    },
};
use itertools::Itertools;
use ruff_db::{
    diagnostic::{Annotation, Span},
    parsed::parsed_module,
};
use ruff_python_ast as ast;
use ruff_python_codegen::{Generator, Indentation};
use ruff_source_file::LineEnding;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use ty_python_core::definition::{Definition, DefinitionKind};

pub(crate) fn check_function_definition<'db>(
    context: &InferContext<'db, '_>,
    definition: Definition<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    let db = context.db();

    let Some(function_type) = infer_definition_types(db, definition).function_type(definition)
    else {
        return;
    };

    let last_definition = function_type.literal(db).last_definition;
    let signature = last_definition.raw_signature(db, ReturnCallableTypeVarScope::Public);

    check_method_receiver(context, last_definition, &signature, file_expression_type);
    check_legacy_positional_only_convention(context, last_definition, &signature);
    check_legacy_typevar_defaults(context, last_definition, &signature, file_expression_type);
    check_legacy_typevar_ordering(context, last_definition, &signature, file_expression_type);
}

fn check_method_receiver<'db>(
    context: &InferContext<'db, '_>,
    last_definition: OverloadLiteral<'db>,
    signature: &Signature<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    let db = context.db();
    let method_name = last_definition.name(db);
    let Some(receiver_parameter) = signature.parameters().get(0) else {
        return;
    };

    if last_definition.is_overload(db)
        || last_definition.has_known_decorator(db, FunctionDecorators::NO_TYPE_CHECK)
        || method_name == "_generate_next_value_"
        || (!last_definition.has_implicit_receiver(db) && method_name != "__new__")
        || receiver_parameter.inferred_annotation
        || !(receiver_parameter.is_positional()
            || receiver_parameter.is_variadic()
            || receiver_parameter.is_keyword_variadic())
    {
        return;
    }

    let Some(enclosing_class) = last_definition
        .body_scope(db)
        .class_definition_of_method(db)
        .and_then(|class_definition| original_class_type(db, class_definition))
        .and_then(|class| class.as_static())
    else {
        return;
    };

    if enclosing_class.is_protocol(db) {
        return;
    }

    let node = last_definition.node(db, context.file(), context.module());
    let Some(annotation) = node
        .parameters
        .iter()
        .next()
        .and_then(ast::AnyParameterRef::annotation)
    else {
        return;
    };

    let annotated_receiver_type = receiver_parameter.annotated_type();
    if receiver_parameter.is_variadic()
        && matches!(
            annotated_receiver_type,
            Type::TypeVar(typevar)
                if typevar.paramspec_attr(db) == Some(ParamSpecAttrKind::Args)
        )
    {
        return;
    }
    let variadic_receiver = if receiver_parameter.is_variadic() {
        variadic_receiver_type(db, annotation, file_expression_type)
    } else {
        None
    };
    let invalid_variadic_receiver =
        matches!(variadic_receiver, Some(VariadicReceiverType::Invalid));
    let raw_receiver_type = match variadic_receiver {
        Some(VariadicReceiverType::Type(receiver_type)) => receiver_type,
        Some(VariadicReceiverType::Invalid) | None => annotated_receiver_type,
    };
    let receiver_type = raw_receiver_type.resolve_type_alias(db);

    if receiver_type.is_never()
        || (enclosing_class.known(db) == Some(KnownClass::Str)
            && receiver_type == Type::literal_string())
    {
        return;
    }

    let class_object = Type::from(enclosing_class);
    let expected_receiver = if last_definition.is_classmethod(db) || method_name == "__new__" {
        class_object
    } else {
        class_object.to_instance(db).unwrap_or_else(Type::unknown)
    };
    let typing_self_type = class_object.to_instance(db).unwrap_or_else(Type::unknown);
    let concrete_receiver_type = receiver_type
        .bind_self_typevars(db, typing_self_type)
        .resolve_type_alias(db);
    let concrete_receiver_type = match concrete_receiver_type {
        Type::TypeVar(typevar) => match typevar.typevar(db).bound_or_constraints(db) {
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound.top_materialization(db),
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                constraints.as_type(db).top_materialization(db)
            }
            None => Type::object(),
        },
        _ => concrete_receiver_type.top_materialization(db),
    };
    if is_protocol_receiver_type(db, receiver_type)
        || is_protocol_receiver_type(db, concrete_receiver_type)
    {
        return;
    }

    if let Some(accepts_receiver) = protocol_class_union_accepts_receiver(
        db,
        annotation,
        expected_receiver,
        typing_self_type,
        file_expression_type,
    ) {
        if accepts_receiver {
            return;
        }
    } else if !invalid_variadic_receiver
        && (expected_receiver.is_assignable_to(db, concrete_receiver_type)
            || (receiver_parameter.is_positional()
                && !matches!(receiver_type, Type::TypeVar(_))
                && signature.can_bind_self_to(db, expected_receiver)))
    {
        return;
    }

    if let Some(builder) = context.report_lint(&INVALID_METHOD_RECEIVER, annotation) {
        let receiver = if any_over_type(db, annotated_receiver_type, false, |ty| ty.is_todo()) {
            Generator::new(&Indentation::default(), LineEnding::default()).expr(annotation)
        } else {
            receiver_type.display(db).to_string()
        };
        builder.into_diagnostic(format_args!(
            "Method receiver type `{receiver}` cannot accept `{expected}`",
            expected = expected_receiver.display(db),
        ));
    }
}

#[derive(Clone, Copy)]
enum VariadicReceiverType<'db> {
    Type(Type<'db>),
    Invalid,
}

fn variadic_receiver_type<'db>(
    db: &'db dyn crate::Db,
    annotation: &ast::Expr,
    file_expression_type: &dyn Fn(&ast::Expr) -> Type<'db>,
) -> Option<VariadicReceiverType<'db>> {
    let tuple_annotation = match annotation {
        ast::Expr::Subscript(subscript)
            if file_expression_type(&subscript.value)
                == Type::SpecialForm(SpecialFormType::Unpack) =>
        {
            &*subscript.slice
        }
        ast::Expr::Starred(starred) => &*starred.value,
        _ => return None,
    };
    let tuple = file_expression_type(tuple_annotation)
        .resolve_type_alias(db)
        .exact_tuple_instance_spec(db)?;
    Some(match tuple.as_ref() {
        Tuple::Variable(tuple)
            if tuple.prefix_elements().is_empty() && tuple.suffix_elements().is_empty() =>
        {
            VariadicReceiverType::Type(*tuple.variable_element())
        }
        Tuple::Fixed(_) | Tuple::Variable(_) => VariadicReceiverType::Invalid,
    })
}

fn is_protocol_receiver_type(db: &dyn crate::Db, receiver_type: Type<'_>) -> bool {
    match receiver_type {
        Type::ProtocolInstance(_) => true,
        Type::ClassLiteral(class) => class.is_protocol(db),
        Type::SubclassOf(subclass_of) => subclass_of
            .subclass_of()
            .into_class(db)
            .is_some_and(|class| class.class_literal(db).is_protocol(db)),
        _ => false,
    }
}

fn protocol_class_union_accepts_receiver<'db>(
    db: &'db dyn crate::Db,
    annotation: &ast::Expr,
    expected_receiver: Type<'db>,
    expected_instance: Type<'db>,
    file_expression_type: &dyn Fn(&ast::Expr) -> Type<'db>,
) -> Option<bool> {
    // `type[Protocol]` currently lowers to a TODO type, so preserve the union's protocol members
    // from the annotation rather than relying on class-object assignability.
    ProtocolClassUnionChecker {
        db,
        expected_receiver,
        expected_instance,
        receiver_is_class_object: expected_receiver
            .is_subtype_of(db, KnownClass::Type.to_instance(db)),
        seen_aliases: FxHashSet::default(),
        seen_typevars: FxHashSet::default(),
    }
    .member_compatibility(
        annotation,
        ReceiverAnnotationResolver::File(file_expression_type),
    )
    .filter(|compatibility| compatibility.contains_protocol_class)
    .map(|compatibility| compatibility.accepts_receiver)
}

#[derive(Clone, Copy)]
struct ProtocolClassUnionCompatibility {
    contains_protocol_class: bool,
    accepts_receiver: bool,
}

impl ProtocolClassUnionCompatibility {
    fn union(self, other: Self) -> Self {
        Self {
            contains_protocol_class: self.contains_protocol_class || other.contains_protocol_class,
            accepts_receiver: self.accepts_receiver || other.accepts_receiver,
        }
    }
}

#[derive(Clone, Copy)]
enum ReceiverAnnotationResolver<'db, 'a> {
    File(&'a dyn Fn(&ast::Expr) -> Type<'db>),
    Definition {
        definition: Definition<'db>,
        alias: Option<TypeAliasType<'db>>,
    },
}

impl<'db> ReceiverAnnotationResolver<'db, '_> {
    fn expression_type(self, db: &'db dyn crate::Db, expression: &ast::Expr) -> Type<'db> {
        if let Self::Definition {
            definition,
            alias: Some(alias),
        } = self
            && let ast::Expr::Name(name) = expression
            && let Some(specialization) = alias.specialization(db)
        {
            let module = parsed_module(db, definition.file(db)).load(db);
            if let DefinitionKind::TypeAlias(type_alias) = definition.kind(db)
                && let Some(type_params) = type_alias.node(&module).type_params.as_deref()
                && let Some(index) = type_params
                    .iter()
                    .position(|type_param| type_param.name().id == name.id)
                && let Some(ty) = specialization.types(db).get(index)
            {
                return *ty;
            }
        }

        match self {
            Self::File(file_expression_type) => file_expression_type(expression),
            Self::Definition { definition, alias } => {
                let ty = definition_expression_type(db, definition, expression);
                alias.map_or(ty, |alias| alias.apply_function_specialization(db, ty))
            }
        }
    }
}

struct ProtocolClassUnionChecker<'db> {
    db: &'db dyn crate::Db,
    expected_receiver: Type<'db>,
    expected_instance: Type<'db>,
    receiver_is_class_object: bool,
    seen_aliases: FxHashSet<TypeAliasType<'db>>,
    seen_typevars: FxHashSet<TypeVarInstance<'db>>,
}

impl<'db> ProtocolClassUnionChecker<'db> {
    fn union_compatibility(
        &mut self,
        annotation: &ast::Expr,
        annotation_type: Type<'db>,
        resolver: ReceiverAnnotationResolver<'db, '_>,
    ) -> Option<ProtocolClassUnionCompatibility> {
        if matches!(annotation, ast::Expr::Name(_) | ast::Expr::Attribute(_))
            && let Some(compatibility) = self.semantic_union_compatibility(annotation_type)
        {
            return Some(compatibility);
        }
        if let ast::Expr::BinOp(binary) = annotation
            && binary.op == ast::Operator::BitOr
        {
            return Some(
                self.member_compatibility(&binary.left, resolver)?
                    .union(self.member_compatibility(&binary.right, resolver)?),
            );
        }

        if let ast::Expr::Subscript(subscript) = annotation {
            match resolver.expression_type(self.db, &subscript.value) {
                Type::SpecialForm(SpecialFormType::Union) => {
                    let mut elements: Box<dyn Iterator<Item = &ast::Expr>> = match &*subscript.slice
                    {
                        ast::Expr::Tuple(tuple) => Box::new(tuple.iter()),
                        element => Box::new(std::iter::once(element)),
                    };
                    let mut compatibility =
                        self.member_compatibility(elements.next()?, resolver)?;
                    for element in elements {
                        compatibility =
                            compatibility.union(self.member_compatibility(element, resolver)?);
                    }
                    return Some(compatibility);
                }
                Type::SpecialForm(SpecialFormType::Optional) => {
                    return Some(
                        self.member_compatibility(&subscript.slice, resolver)?
                            .union(ProtocolClassUnionCompatibility {
                                contains_protocol_class: false,
                                accepts_receiver: false,
                            }),
                    );
                }
                Type::SpecialForm(SpecialFormType::Annotated) => {
                    let ast::Expr::Tuple(tuple) = &*subscript.slice else {
                        return None;
                    };
                    let inner = tuple.elts.first()?;
                    return self.member_compatibility(inner, resolver);
                }
                _ => {}
            }
        }

        let alias = match annotation_type {
            Type::TypeAlias(alias)
            | Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) => Some(alias),
            _ => None,
        };
        if let Some(alias) = alias {
            if !self.seen_aliases.insert(alias) {
                return None;
            }
            let compatibility = self.alias_compatibility(alias);
            self.seen_aliases.remove(&alias);
            return compatibility;
        }

        let typevar = match annotation_type {
            Type::TypeVar(bound_typevar) => Some(bound_typevar.typevar(self.db)),
            Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => Some(typevar),
            _ => None,
        };
        if let Some(typevar) = typevar {
            if !self.seen_typevars.insert(typevar) {
                return None;
            }
            let compatibility = self.typevar_compatibility(typevar);
            self.seen_typevars.remove(&typevar);
            return compatibility;
        }

        None
    }

    fn alias_compatibility(
        &mut self,
        alias: TypeAliasType<'db>,
    ) -> Option<ProtocolClassUnionCompatibility> {
        let definition = alias.definition(self.db);
        let module = parsed_module(self.db, definition.file(self.db)).load(self.db);
        let DefinitionKind::TypeAlias(type_alias) = definition.kind(self.db) else {
            return None;
        };
        let value = &type_alias.node(&module).value;
        self.member_compatibility(
            value,
            ReceiverAnnotationResolver::Definition {
                definition,
                alias: Some(alias),
            },
        )
    }

    fn typevar_compatibility(
        &mut self,
        typevar: TypeVarInstance<'db>,
    ) -> Option<ProtocolClassUnionCompatibility> {
        let definition = typevar.definition(self.db)?;
        let module = parsed_module(self.db, definition.file(self.db)).load(self.db);
        let resolver = ReceiverAnnotationResolver::Definition {
            definition,
            alias: None,
        };
        match definition.kind(self.db) {
            DefinitionKind::TypeVar(typevar) => {
                let bound = typevar.node(&module).bound.as_ref()?;
                self.member_compatibility(bound, resolver)
            }
            DefinitionKind::Assignment(assignment) => {
                let call = assignment.value(&module).as_call_expr()?;
                if let Some(bound) = call.arguments.find_keyword("bound") {
                    self.member_compatibility(&bound.value, resolver)
                } else {
                    let mut constraints = call.arguments.args.iter().skip(1);
                    let mut compatibility =
                        self.member_compatibility(constraints.next()?, resolver)?;
                    for constraint in constraints {
                        compatibility =
                            compatibility.union(self.member_compatibility(constraint, resolver)?);
                    }
                    Some(compatibility)
                }
            }
            _ => None,
        }
    }

    fn member_compatibility(
        &mut self,
        annotation: &ast::Expr,
        resolver: ReceiverAnnotationResolver<'db, '_>,
    ) -> Option<ProtocolClassUnionCompatibility> {
        let annotation_type = resolver.expression_type(self.db, annotation);
        if let Some(compatibility) = self.union_compatibility(annotation, annotation_type, resolver)
        {
            return Some(compatibility);
        }
        if matches!(annotation, ast::Expr::Name(_) | ast::Expr::Attribute(_))
            && let Some(compatibility) = self.semantic_union_compatibility(annotation_type)
        {
            return Some(compatibility);
        }

        if let ast::Expr::Subscript(subscript) = annotation {
            let is_type_subscript = match resolver.expression_type(self.db, &subscript.value) {
                Type::ClassLiteral(class) => class.known(self.db) == Some(KnownClass::Type),
                Type::SpecialForm(SpecialFormType::Type) => true,
                _ => false,
            };
            if is_type_subscript {
                let protocol_instance =
                    self.protocol_instance_from_annotation(&subscript.slice, resolver);
                if let Some(protocol_instance) = protocol_instance {
                    return Some(ProtocolClassUnionCompatibility {
                        contains_protocol_class: true,
                        accepts_receiver: self.receiver_is_class_object
                            && self
                                .expected_instance
                                .is_assignable_to(self.db, protocol_instance),
                    });
                }
            }
        }

        Some(ProtocolClassUnionCompatibility {
            contains_protocol_class: false,
            accepts_receiver: self
                .expected_receiver
                .is_assignable_to(self.db, annotation_type.resolve_type_alias(self.db)),
        })
    }

    fn semantic_union_compatibility(
        &self,
        annotation_type: Type<'db>,
    ) -> Option<ProtocolClassUnionCompatibility> {
        let annotation_type = annotation_type.resolve_type_alias(self.db);
        if let Type::Union(union) = annotation_type {
            let mut elements = union.elements(self.db).iter().copied();
            let mut compatibility = self.semantic_member_compatibility(elements.next()?)?;
            for element in elements {
                compatibility = compatibility.union(self.semantic_member_compatibility(element)?);
            }
            return compatibility
                .contains_protocol_class
                .then_some(compatibility);
        }
        let compatibility = self.semantic_member_compatibility(annotation_type)?;
        compatibility
            .contains_protocol_class
            .then_some(compatibility)
    }

    fn semantic_member_compatibility(
        &self,
        annotation_type: Type<'db>,
    ) -> Option<ProtocolClassUnionCompatibility> {
        let protocol_instance = match annotation_type {
            Type::SubclassOf(subclass_of)
                if subclass_of
                    .subclass_of()
                    .into_class(self.db)
                    .is_some_and(|class| class.class_literal(self.db).is_protocol(self.db)) =>
            {
                Some(subclass_of.to_instance(self.db))
            }
            Type::ClassLiteral(class) if class.is_protocol(self.db) => {
                Type::from(class).to_instance(self.db)
            }
            _ => None,
        };
        Some(if let Some(protocol_instance) = protocol_instance {
            ProtocolClassUnionCompatibility {
                contains_protocol_class: true,
                accepts_receiver: self.receiver_is_class_object
                    && self
                        .expected_instance
                        .is_assignable_to(self.db, protocol_instance),
            }
        } else {
            ProtocolClassUnionCompatibility {
                contains_protocol_class: false,
                accepts_receiver: self
                    .expected_receiver
                    .is_assignable_to(self.db, annotation_type),
            }
        })
    }

    fn protocol_instance_from_annotation(
        &self,
        annotation: &ast::Expr,
        resolver: ReceiverAnnotationResolver<'db, '_>,
    ) -> Option<Type<'db>> {
        if let Some(protocol) =
            self.protocol_instance_from_type(resolver.expression_type(self.db, annotation))
        {
            return Some(protocol);
        }

        if let ast::Expr::BinOp(binary) = annotation
            && binary.op == ast::Operator::BitOr
        {
            return Some(UnionType::from_two_elements(
                self.db,
                self.protocol_instance_from_annotation(&binary.left, resolver)?,
                self.protocol_instance_from_annotation(&binary.right, resolver)?,
            ));
        }

        let ast::Expr::Subscript(subscript) = annotation else {
            return None;
        };
        match resolver.expression_type(self.db, &subscript.value) {
            Type::SpecialForm(SpecialFormType::Union) => {
                let mut elements: Box<dyn Iterator<Item = &ast::Expr>> = match &*subscript.slice {
                    ast::Expr::Tuple(tuple) => Box::new(tuple.iter()),
                    element => Box::new(std::iter::once(element)),
                };
                let mut protocol =
                    self.protocol_instance_from_annotation(elements.next()?, resolver)?;
                for element in elements {
                    protocol = UnionType::from_two_elements(
                        self.db,
                        protocol,
                        self.protocol_instance_from_annotation(element, resolver)?,
                    );
                }
                return Some(protocol);
            }
            Type::SpecialForm(SpecialFormType::Annotated) => {
                let ast::Expr::Tuple(tuple) = &*subscript.slice else {
                    return None;
                };
                return self.protocol_instance_from_annotation(tuple.elts.first()?, resolver);
            }
            _ => {}
        }

        let Type::ClassLiteral(protocol_class) =
            resolver.expression_type(self.db, &subscript.value)
        else {
            return None;
        };
        if !protocol_class.is_protocol(self.db) {
            return None;
        }

        let arguments: Box<dyn Iterator<Item = &ast::Expr>> = match &*subscript.slice {
            ast::Expr::Tuple(tuple) => Box::new(tuple.iter()),
            argument => Box::new(std::iter::once(argument)),
        };
        let argument_types: Vec<_> = arguments
            .map(|argument| {
                let ty = resolver
                    .expression_type(self.db, argument)
                    .resolve_type_alias(self.db);
                ty.to_instance(self.db).unwrap_or(ty)
            })
            .collect();
        let generic_context = protocol_class.generic_context(self.db)?;
        if argument_types.len() != generic_context.variables(self.db).len() {
            return None;
        }
        let protocol = protocol_class.apply_specialization(self.db, |generic_context| {
            generic_context.specialize_partial(self.db, argument_types.into_iter().map(Some))
        });
        Some(Type::instance(self.db, protocol))
    }

    fn protocol_instance_from_type(&self, annotation_type: Type<'db>) -> Option<Type<'db>> {
        match annotation_type.resolve_type_alias(self.db) {
            protocol @ Type::ProtocolInstance(_) => Some(protocol),
            Type::ClassLiteral(class) if class.is_protocol(self.db) => {
                Type::from(class).to_instance(self.db)
            }
            Type::GenericAlias(alias) if alias.origin(self.db).is_protocol(self.db) => {
                Type::from(alias).to_instance(self.db)
            }
            Type::Union(union) => UnionType::try_from_elements(
                self.db,
                union
                    .elements(self.db)
                    .iter()
                    .map(|element| self.protocol_instance_from_type(*element)),
            ),
            _ => None,
        }
    }
}

/// Check for invalid applications of the pre-PEP-570 positional-only parameter convention.
fn check_legacy_positional_only_convention<'db>(
    context: &InferContext<'db, '_>,
    last_definition: OverloadLiteral<'db>,
    signature: &Signature<'db>,
) {
    let db = context.db();
    let node = last_definition.node(db, context.file(), context.module());
    let ast_parameters = &node.parameters;

    // If the function has any PEP-570 positional-only parameters,
    // assume that `__`-prefixed parameters are not meant to be positional-only
    if !ast_parameters.posonlyargs.is_empty() {
        return;
    }
    let parsed_parameters = signature.parameters();
    let mut previous_non_positional_only: Option<&ast::ParameterWithDefault> = None;

    for (param_node, param) in std::iter::zip(ast_parameters, parsed_parameters) {
        let ast::AnyParameterRef::NonVariadic(param_node) = param_node else {
            continue;
        };
        if param.is_positional_only() {
            continue;
        }

        // Valid uses of the PEP-484 positional-only convention will have been detected as such
        // in the first iteration over this scope, so `param.is_positional_only()` will return `true`
        // for those. We only get here for invalid uses of the PEP-484 positional-only convention.
        if param_node.uses_pep_484_positional_only_convention() {
            let Some(builder) =
                context.report_lint(&INVALID_LEGACY_POSITIONAL_PARAMETER, param_node.name())
            else {
                continue;
            };
            let mut diagnostic = builder.into_diagnostic(
                "Invalid use of the legacy convention \
                    for positional-only parameters",
            );
            diagnostic.set_primary_message(
                "Parameter name begins with `__` but will not be treated as positional-only",
            );
            diagnostic.info(
                "A parameter can only be positional-only \
                    if it precedes all positional-or-keyword parameters",
            );
            if let Some(earlier_node) = previous_non_positional_only {
                diagnostic.annotate(
                    context
                        .secondary(earlier_node.name())
                        .message("Prior parameter here was positional-or-keyword"),
                );
            }
        } else if previous_non_positional_only.is_none() {
            previous_non_positional_only = Some(param_node);
        }
    }
}

/// Check whether any legacy `TypeVar` used in a function signature has a default
/// that references an out-of-scope type variable.
///
/// This check mirrors the class-level check at `report_invalid_typevar_default_reference`,
/// but for function/method generic contexts.
fn check_legacy_typevar_defaults<'db>(
    context: &InferContext<'db, '_>,
    last_definition: OverloadLiteral<'db>,
    signature: &Signature<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    let db = context.db();

    let Some(generic_context) = signature.generic_context else {
        return;
    };

    let typevars = generic_context
        .variables(db)
        .map(|bound_tvar| bound_tvar.typevar(db));

    for (i, typevar) in typevars.clone().enumerate() {
        // Only check legacy TypeVars; PEP 695 type parameters are already validated
        // by `check_default_for_outer_scope_typevars` in the type parameter scope.
        if !matches!(
            typevar.kind(db),
            TypeVarKind::Legacy | TypeVarKind::Pep613Alias | TypeVarKind::ParamSpec
        ) {
            continue;
        }

        let Some(default_ty) = typevar.default_type(db) else {
            continue;
        };

        let first_bad_tvar = find_over_type(db, default_ty, false, |t| {
            let tvar = match t {
                Type::TypeVar(tvar) => tvar.typevar(db),
                Type::KnownInstance(KnownInstanceType::TypeVar(tvar)) => tvar,
                _ => return None,
            };
            if !typevars.clone().take(i).contains(&tvar) {
                Some(tvar)
            } else {
                None
            }
        });

        let Some(bad_typevar) = first_bad_tvar else {
            continue;
        };

        let is_later_in_list = typevars.clone().skip(i).contains(&bad_typevar);
        let node = last_definition.node(db, context.file(), context.module());

        let primary_range =
            find_typevar_annotation_range(context, node, typevar, file_expression_type);

        let Some(builder) = context.report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, primary_range)
        else {
            continue;
        };
        let typevar_name = typevar.name(db);
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Invalid use of type variable `{typevar_name}`",
        ));

        if is_later_in_list {
            diagnostic.set_primary_message(format_args!(
                "Default of `{typevar_name}` references later type parameter `{}`",
                bad_typevar.name(db),
            ));
            diagnostic.set_concise_message(format_args!(
                "Invalid use of type variable `{typevar_name}`: default of `{typevar_name}` \
                    refers to later parameter `{}`",
                bad_typevar.name(db)
            ));
        } else {
            diagnostic.set_primary_message(format_args!(
                "Default of `{typevar_name}` references out-of-scope type variable `{}`",
                bad_typevar.name(db),
            ));
            diagnostic.set_concise_message(format_args!(
                "Invalid use of type variable `{typevar_name}`: default of `{typevar_name}` \
                    refers to out-of-scope type variable `{}`",
                bad_typevar.name(db)
            ));
        }

        if let Some(typevar_definition) = typevar.definition(db) {
            let file = typevar_definition.file(db);
            diagnostic.annotate(
                Annotation::secondary(Span::from(
                    typevar_definition.full_range(db, &parsed_module(db, file).load(db)),
                ))
                .message(format_args!("`{typevar_name}` defined here")),
            );
        }

        diagnostic.info("See https://typing.python.org/en/latest/spec/generics.html#scoping-rules");
    }
}

fn find_typevar_annotation_range<'db>(
    context: &InferContext<'db, '_>,
    node: &ast::StmtFunctionDef,
    typevar: TypeVarInstance<'db>,
    file_expression_type: impl Fn(&ast::Expr) -> Type<'db>,
) -> TextRange {
    let db = context.db();
    let typevar_id = typevar.identity(db);

    node.parameters
        .iter()
        .filter_map(ast::AnyParameterRef::annotation)
        .chain(node.returns.as_deref())
        .find(|ann| file_expression_type(ann).references_typevar(db, typevar_id))
        .map(Ranged::range)
        .unwrap_or_else(|| node.name.range())
}

/// Check that legacy `TypeVar`s without defaults don't follow `TypeVar`s with defaults
/// in a function's generic context.
///
/// This mirrors the class-level check using `report_invalid_type_param_order`, but for
/// function/method generic contexts using the `invalid-type-variable-default` lint.
fn check_legacy_typevar_ordering<'db>(
    context: &InferContext<'db, '_>,
    last_definition: OverloadLiteral<'db>,
    signature: &Signature<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    struct State<'db> {
        typevar_with_default: TypeVarInstance<'db>,
        invalid_later_tvars: Vec<TypeVarInstance<'db>>,
    }

    let db = context.db();

    let Some(generic_context) = signature.generic_context else {
        return;
    };

    let mut state: Option<State<'db>> = None;

    for bound_typevar in generic_context.variables(db) {
        let typevar = bound_typevar.typevar(db);

        // Only check legacy TypeVars; PEP 695 ordering is validated by the parser.
        if !matches!(
            typevar.kind(db),
            TypeVarKind::Legacy | TypeVarKind::Pep613Alias | TypeVarKind::ParamSpec
        ) {
            continue;
        }

        let has_default = typevar.default_type(db).is_some();

        if let Some(state) = state.as_mut() {
            if !has_default {
                state.invalid_later_tvars.push(typevar);
            }
        } else if has_default {
            state = Some(State {
                typevar_with_default: typevar,
                invalid_later_tvars: vec![],
            });
        }
    }

    let Some(state) = state else {
        return;
    };

    if state.invalid_later_tvars.is_empty() {
        return;
    }

    let node = last_definition.node(db, context.file(), context.module());

    let primary_range = find_typevar_annotation_range(
        context,
        node,
        state.invalid_later_tvars[0],
        file_expression_type,
    );

    let Some(builder) = context.report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, primary_range) else {
        return;
    };

    let mut diagnostic = builder.into_diagnostic(
        "Type parameters without defaults cannot follow type parameters with defaults",
    );

    let typevar_with_default_name = state.typevar_with_default.name(db);

    diagnostic.set_concise_message(format_args!(
        "Type parameter `{}` without a default cannot follow \
            earlier parameter `{typevar_with_default_name}` with a default",
        state.invalid_later_tvars[0].name(db),
    ));

    if let [single_typevar] = &*state.invalid_later_tvars {
        diagnostic.set_primary_message(format_args!(
            "Type variable `{}` does not have a default",
            single_typevar.name(db),
        ));
    } else {
        let later_typevars =
            format_enumeration(state.invalid_later_tvars.iter().map(|tv| tv.name(db)));
        diagnostic.set_primary_message(format_args!(
            "Type variables {later_typevars} do not have defaults",
        ));
    }

    let secondary_range = find_typevar_annotation_range(
        context,
        node,
        state.typevar_with_default,
        file_expression_type,
    );

    diagnostic.annotate(context.secondary(secondary_range).message(format_args!(
        "Earlier TypeVar `{typevar_with_default_name}` has a default"
    )));

    for tvar in [state.typevar_with_default, state.invalid_later_tvars[0]] {
        let Some(definition) = tvar.definition(db) else {
            continue;
        };
        let file = definition.file(db);
        diagnostic.annotate(
            Annotation::secondary(Span::from(
                definition.full_range(db, &parsed_module(db, file).load(db)),
            ))
            .message(format_args!("`{}` defined here", tvar.name(db))),
        );
    }
}
