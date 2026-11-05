use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, AnyNodeRef, HasNodeIndex, NodeIndex, PythonVersion};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use strum::IntoEnumIterator;

use super::{ArgumentsIter, TypeInferenceBuilder};
use crate::TypeQualifiers;
use crate::types::call::CallError;
use crate::types::class::{ClassLiteral, DynamicTypedDictAnchor, DynamicTypedDictLiteral};
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, INVALID_TYPE_FORM, MISSING_ARGUMENT, TOO_MANY_POSITIONAL_ARGUMENTS,
    UNKNOWN_ARGUMENT, report_mismatched_type_name,
};
use crate::types::special_form::TypeQualifier;
use crate::types::typed_dict::{
    ConstructorKeys, TypedDictArgument, TypedDictOpenness, TypedDictSchema,
    functional_typed_dict_field, typed_dict_with_relaxed_keys, validate_typed_dict_constructor,
    validate_typed_dict_constructor_keywords, validate_typed_dict_dict_literal,
};
use crate::types::visitor::any_over_type_expanding_aliases;
use crate::types::{
    ClassType, IntersectionType, KnownClass, Type, TypeAndQualifiers, TypeContext, TypedDictType,
    TypingModule, any_over_type,
};
use ty_python_core::ExpressionNodeKey;
use ty_python_core::definition::Definition;

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer a `TypedDict(name, fields)` call expression.
    ///
    /// This method *does not* call `infer_expression` on the object being called;
    /// it is assumed that the type for this AST node has already been inferred before this method is called.
    pub(super) fn infer_typeddict_call_expression(
        &mut self,
        call_expr: &ast::ExprCall,
        definition: Option<Definition<'db>>,
        typed_dict_module: TypingModule,
    ) -> Type<'db> {
        let env = self.program_environment();
        let db = self.db();
        let ast::Arguments {
            args,
            keywords,
            range: _,
            node_index: _,
        } = &call_expr.arguments;

        let starred_arguments: SmallVec<[&ast::Expr; 1]> =
            args.iter().filter(|arg| arg.is_starred_expr()).collect();
        let double_starred_arguments: SmallVec<[&ast::Keyword; 1]> =
            keywords.iter().filter(|kw| kw.arg.is_none()).collect();

        // The fallback type reflects the fact that if the call were successful,
        // it would return a class that is a subclass of `Mapping[str, object]`
        // with an unknown set of fields.
        let fallback = || {
            let spec = &[KnownClass::Str.to_instance(db, env), Type::object()];
            let str_object_map = KnownClass::Mapping.to_specialized_subclass_of(db, env, spec);
            IntersectionType::from_two_elements(db, env, str_object_map, Type::unknown())
        };

        // Emit diagnostic for unsupported variadic arguments.
        match (&*starred_arguments, &*double_starred_arguments) {
            ([], []) => {}
            (starred, []) => {
                if let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, starred[0])
                {
                    let mut diagnostic = builder.into_diagnostic(
                        "Variadic positional arguments are not supported in `TypedDict()` calls",
                    );
                    for arg in &starred[1..] {
                        diagnostic.annotate(self.context.secondary(arg));
                    }
                }
            }
            ([], double_starred) => {
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_ARGUMENT_TYPE, double_starred[0])
                {
                    let mut diagnostic = builder.into_diagnostic(
                        "Variadic keyword arguments are not supported in `TypedDict()` calls",
                    );
                    for arg in &double_starred[1..] {
                        diagnostic.annotate(self.context.secondary(arg));
                    }
                }
            }
            _ => {
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_ARGUMENT_TYPE, starred_arguments[0])
                {
                    let mut diagnostic = builder.into_diagnostic(
                        "Variadic positional and keyword arguments are not supported in `TypedDict()` calls",
                    );
                    for arg in &starred_arguments[1..] {
                        diagnostic.annotate(self.context.secondary(arg));
                    }
                    for arg in &double_starred_arguments {
                        diagnostic.annotate(self.context.secondary(arg));
                    }
                }
            }
        }

        let mut total = true;
        let mut closed = false;
        let mut extra_items = None;
        let supports_pep_728 = self.in_stub()
            || typed_dict_module == TypingModule::TypingExtensions
            || self.program_environment().python_version(db) >= PythonVersion::PY315;

        for kw in keywords {
            let Some(arg) = &kw.arg else {
                continue;
            };

            if !supports_pep_728
                && matches!(&**arg, "closed" | "extra_items")
                && let Some(builder) = self.context.report_lint(&UNKNOWN_ARGUMENT, kw)
            {
                builder.into_diagnostic(format_args!(
                    "The `{arg}` parameter of `typing.TypedDict` was added in Python 3.15"
                ));
            }

            match &**arg {
                arg_name @ ("total" | "closed") => {
                    let kw_type = self.infer_expression(&kw.value, TypeContext::default());
                    if !kw.value.is_boolean_literal_expr()
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, &kw.value)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Invalid argument to parameter `{arg_name}` of `TypedDict()`"
                        ));
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Expected either `True` or `False`, got object of type `{}`",
                            kw_type.display(db, env)
                        ));
                    }

                    if arg_name == "total" {
                        if kw_type.bool(db, env).is_always_false() {
                            total = false;
                        } else if !kw_type.bool(db, env).is_always_true() {
                            total = true;
                        }
                    } else {
                        closed = kw_type.bool(db, env).is_always_true();
                    }
                }
                "extra_items" => {
                    if definition.is_none() {
                        let annotation = self.infer_extra_items_kwarg(&kw.value);
                        extra_items = Some(TypedDictOpenness::extra(
                            db,
                            annotation.inner_type(),
                            annotation.qualifiers().contains(TypeQualifiers::READ_ONLY),
                        ));
                    }
                }
                unknown_kwarg => {
                    self.infer_expression(&kw.value, TypeContext::default());
                    if let Some(builder) = self.context.report_lint(&UNKNOWN_ARGUMENT, kw) {
                        builder.into_diagnostic(format_args!(
                            "Argument `{unknown_kwarg}` does not match any known parameter of function `TypedDict`",
                        ));
                    }
                }
            }
        }

        if let Some(extra_items_kwarg) = call_expr.arguments.find_keyword("extra_items")
            && call_expr.arguments.find_keyword("closed").is_some()
            && let Some(builder) = self
                .context
                .report_lint(&INVALID_ARGUMENT_TYPE, extra_items_kwarg)
        {
            builder.into_diagnostic("`closed` and `extra_items` cannot both be specified");
        }

        if !starred_arguments.is_empty() || !double_starred_arguments.is_empty() {
            for arg in args {
                self.infer_expression(arg, TypeContext::default());
            }
            return fallback();
        }

        if args.len() > 2
            && let Some(builder) = self
                .context
                .report_lint(&TOO_MANY_POSITIONAL_ARGUMENTS, &args[2])
        {
            builder.into_diagnostic(format_args!(
                "Too many positional arguments to function `TypedDict`: expected 2, got {}",
                args.len()
            ));
        }

        let Some(name_arg) = args.first() else {
            if let Some(builder) = self.context.report_lint(&MISSING_ARGUMENT, call_expr) {
                builder.into_diagnostic(
                    "No arguments provided for required parameters `typename` \
                    and `fields` of function `TypedDict`",
                );
            }

            return fallback();
        };

        let name_type = self.infer_expression(name_arg, TypeContext::default());

        let Some(fields_arg) = args.get(1) else {
            if let Some(builder) = self.context.report_lint(&MISSING_ARGUMENT, call_expr) {
                builder.into_diagnostic(
                    "No argument provided for required parameter `fields` of function `TypedDict`",
                );
            }
            return fallback();
        };

        for arg in args.iter().skip(2) {
            self.infer_expression(arg, TypeContext::default());
        }

        let name = name_type
            .as_string_literal()
            .map(|literal| literal.value(db));

        if name.is_none()
            && !name_type.is_assignable_to(db, env, KnownClass::Str.to_instance(db, env))
            && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
        {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Invalid argument to parameter `typename` of `TypedDict()`"
            ));
            diagnostic.set_primary_annotation_message(format_args!(
                "Expected `str`, found `{}`",
                name_type.display(db, env)
            ));
        } else if let Some(definition) = definition
            && let Some(assigned_name) = definition.name(db)
            && Some(assigned_name.as_str()) != name
        {
            report_mismatched_type_name(
                &self.context,
                name_arg,
                "TypedDict",
                &assigned_name,
                name,
                name_type,
            );
        }

        let name = name.unwrap_or("<unknown>");

        self.validate_fields_arg(fields_arg);

        if let Some(definition) = definition {
            self.deferred.insert(definition);
        }

        let scope = self.scope();
        let anchor = match definition {
            Some(definition) => DynamicTypedDictAnchor::Definition(definition),
            None => {
                let schema = self.infer_dangling_typeddict_spec(fields_arg, total);

                DynamicTypedDictAnchor::ScopeOffset {
                    scope,
                    offset: self.dynamic_class_scope_offset(call_expr),
                    schema,
                    openness: extra_items.unwrap_or(if closed {
                        TypedDictOpenness::Closed
                    } else {
                        TypedDictOpenness::ImplicitlyOpen
                    }),
                }
            }
        };

        let typeddict = DynamicTypedDictLiteral::new(db, name, anchor, typed_dict_module);
        Type::ClassLiteral(ClassLiteral::DynamicTypedDict(typeddict))
    }

    pub(super) fn infer_typed_dict_expression(
        &mut self,
        dict: &ast::ExprDict,
        typed_dict: TypedDictType<'db>,
        item_types: &mut FxHashMap<NodeIndex, Type<'db>>,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let env = self.program_environment();
        let ast::ExprDict {
            range: _,
            node_index: _,
            items,
        } = dict;

        let key_tcx =
            TypeContext::declared(self.typed_dict_key_expected_type(Type::TypedDict(typed_dict)));

        for item in items {
            let key_ty = self.infer_optional_expression(item.key.as_ref(), key_tcx);
            if let Some((key, key_ty)) = item.key.as_ref().zip(key_ty) {
                item_types.insert(key.node_index().load(), key_ty);
            }

            let value_ty = if let Some(key_ty) = key_ty
                && let Some(key) = key_ty.as_string_literal()
                && let Some(field) = typed_dict.item(self.db(), key.value(self.db()))
            {
                self.infer_expression(&item.value, TypeContext::declared(Some(field.declared_ty)))
            } else if let Some(key_ty) = key_ty {
                if key_ty.is_assignable_to(db, env, KnownClass::Str.to_instance(db, env))
                    && let Some(value_ty) = typed_dict.arbitrary_key_initialization_type(db, env)
                {
                    self.infer_expression(&item.value, TypeContext::declared(Some(value_ty)))
                } else {
                    self.infer_expression(&item.value, TypeContext::default())
                }
            } else {
                self.infer_expression(&item.value, TypeContext::default())
            };

            item_types.insert(item.value.node_index().load(), value_ty);
        }

        let (valid, _) = validate_typed_dict_dict_literal(
            &self.context,
            typed_dict,
            dict,
            AnyNodeRef::from(dict),
            |expr| TypedDictArgument {
                ty: item_types
                    .get(&expr.node_index().load())
                    .copied()
                    .unwrap_or_else(Type::unknown),
                ..self.typed_dict_argument(expr)
            },
        );
        valid.then_some(Type::TypedDict(typed_dict))
    }

    /// Infers and validates a `TypedDict` constructor through one call-binding pipeline.
    ///
    /// Bare generic constructors infer from direct keyword arguments. Other forms use the existing
    /// field-directed validation and bind against the class's default specialization:
    ///
    /// ```python
    /// Box(value=1)        # Box[int]
    /// Box({"value": 1})   # Box[Unknown]
    /// ```
    pub(super) fn infer_typed_dict_constructor<'expr>(
        &mut self,
        callable_type: Type<'db>,
        class: ClassType<'db>,
        call_expression: &'expr ast::ExprCall,
        call_expression_tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let typed_dict = TypedDictType::new(class);
        let arguments = &call_expression.arguments;
        let error_node = AnyNodeRef::from(call_expression.func.as_ref());
        let fallback_ty = callable_type
            .to_instance_approximation(db, env)
            .unwrap_or_else(Type::unknown);
        let generic_class = callable_type
            .as_class_literal()
            .filter(|class| class.generic_context(db).is_some());
        if generic_class.is_some() && arguments.args.is_empty() {
            for keyword in &arguments.keywords {
                if keyword.arg.is_none() && !keyword.value.is_dict_expr() {
                    self.get_or_infer_expression(&keyword.value, TypeContext::default());
                }
            }
        }
        let generic_keys = generic_class.and_then(|class| {
            self.generic_typed_dict_constructor_keys(class, arguments, call_expression_tcx)
        });
        let can_infer = generic_keys.is_some();
        let keys = generic_keys.unwrap_or_else(|| {
            self.prepare_typed_dict_constructor(typed_dict, arguments, error_node)
        });

        let mut call_arguments = self.prepare_call_arguments(arguments);
        let binding_callable = if generic_class.is_some() && !can_infer {
            Type::from(class.class_literal(db).default_specialization(db))
        } else {
            callable_type
        };
        let bindings =
            self.bindings_for_call(binding_callable)
                .match_parameters(db, env, &call_arguments);

        if can_infer && !bindings.satisfies(|_| true) {
            let keys = self.prepare_typed_dict_constructor(typed_dict, arguments, error_node);
            self.record_typed_dict_constructor_keys(typed_dict, call_expression, keys);
            return fallback_ty;
        }

        let result = self.infer_and_check_argument_types(
            ArgumentsIter::from_ast(arguments),
            &mut call_arguments,
            &mut |builder, (_, expr, tcx)| {
                if can_infer {
                    builder.infer_expression(expr, tcx)
                } else {
                    builder.get_or_infer_expression(expr, tcx)
                }
            },
            bindings,
            call_expression_tcx,
        );

        self.record_typed_dict_constructor_keys(typed_dict, call_expression, keys);

        let return_ty = result.map(|inferred| inferred.into_bindings().return_type(db, env));
        return_ty.unwrap_or_else(|CallError(_, bindings)| {
            if can_infer
                && arguments.keywords.iter().any(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .and_then(|name| typed_dict.item(db, name.id.as_str()))
                        .is_some_and(|field| {
                            !self.expression_type(&keyword.value).is_assignable_to(
                                db,
                                env,
                                field.declared_ty,
                            )
                        })
                })
            {
                self.prepare_typed_dict_constructor(typed_dict, arguments, error_node);
                return fallback_ty;
            }

            bindings.report_diagnostics(&self.context, AnyNodeRef::from(call_expression));
            bindings.return_type(db, env)
        })
    }

    /// Records only the supplied keys a reader cannot infer from the declared type.
    /// For `Movie(year=1979)`, only an optional `year` needs to be recorded.
    fn record_typed_dict_constructor_keys(
        &mut self,
        typed_dict: TypedDictType<'db>,
        call: &ast::ExprCall,
        mut keys: ConstructorKeys<'_>,
    ) {
        let db = self.db();
        keys.retain(|name, source| {
            source.is_some()
                && typed_dict
                    .item(db, name)
                    .is_some_and(|field| !field.is_required())
        });
        if !keys.is_empty() {
            self.typed_dict_constructor_keys
                .insert(ExpressionNodeKey::from(call), keys.into_keys().collect());
        }
    }

    /// Returns the type and known keys of an argument that has already been inferred.
    fn typed_dict_argument(&self, expr: &ast::Expr) -> TypedDictArgument<'_, 'db> {
        TypedDictArgument {
            ty: self.expression_type(expr),
            present_keys: self
                .typed_dict_constructor_keys
                .get(&ExpressionNodeKey::from(expr))
                .map_or(&[], |keys| keys.as_ref()),
        }
    }

    /// Returns the supplied keys when ordinary call binding can infer a generic `TypedDict`.
    ///
    /// Mapping arguments, unresolved nested `TypedDict` fields, and gradual expected types remain
    /// on the field-directed path so sibling arguments cannot force an unsound specialization:
    ///
    /// ```python
    /// Outer(inner=Inner(value=1), marker="x")  # Outer[Unknown]
    /// ```
    ///
    /// TODO: Remove this gate once ordinary generic call inference can safely handle mapping
    /// arguments, nested `TypedDict` fields, and unresolved contextual type arguments.
    fn generic_typed_dict_constructor_keys<'expr>(
        &self,
        class_literal: ClassLiteral<'db>,
        arguments: &'expr ast::Arguments,
        call_expression_tcx: TypeContext<'db>,
    ) -> Option<ConstructorKeys<'expr>> {
        let db = self.db();
        let env = self.program_environment();

        // An inner `Node(value=1)` must retain `Node[Unknown]` when its enclosing
        // `Node(child=...)` cannot infer through the recursive field.
        let has_gradual_class_context = class_literal
            .as_static()
            .zip(call_expression_tcx.annotation)
            .is_some_and(|(class_literal, annotation)| {
                any_over_type(db, env, annotation.resolve_type_alias(db), false, |ty| {
                    ty.resolve_type_alias(db)
                        .specialization_of(db, env, class_literal)
                        .is_some_and(|specialization| {
                            specialization
                                .types(db)
                                .iter()
                                .any(|ty| ty.is_unknown() || ty.has_typevar(db, env))
                        })
                })
            });
        let typed_dict = TypedDictType::new(class_literal.identity_specialization(db));

        if !arguments.args.is_empty() || has_gradual_class_context {
            return None;
        }
        let permits_field_inference = |name: &str| {
            typed_dict.item(db, name).is_none_or(|field| {
                !any_over_type_expanding_aliases(db, env, field.declared_ty, |nested| {
                    nested.is_typed_dict() && nested.has_typevar(db, env)
                })
            })
        };

        let mut keys = ConstructorKeys::new();
        for keyword in &arguments.keywords {
            if let Some(name) = &keyword.arg {
                if !permits_field_inference(name.as_str()) {
                    return None;
                }
                keys.insert(name.id.clone(), Some(AnyNodeRef::from(keyword)));
            } else {
                let unpacked = self
                    .try_expression_type(&keyword.value)?
                    .resolve_type_alias(db)
                    .as_typed_dict()?;
                for (name, field) in unpacked.items(db) {
                    if !field.is_required() || !permits_field_inference(name.as_str()) {
                        return None;
                    }
                    keys.insert(name.clone(), Some(AnyNodeRef::from(keyword)));
                }
            }
        }
        Some(keys)
    }

    /// Prepare a `TypedDict` constructor call before general argument inference.
    ///
    /// This gives constructor values the declared field type as context, then validates the full
    /// call once when needed. A lone positional dict literal is inferred as a `TypedDict`
    /// expression directly, while mixed dict-literal and keyword calls infer the nested key and
    /// value expressions without re-inferring the outer dict literal later during argument
    /// binding.
    pub(super) fn prepare_typed_dict_constructor<'expr>(
        &mut self,
        typed_dict: TypedDictType<'db>,
        arguments: &'expr ast::Arguments,
        error_node: AnyNodeRef<'expr>,
    ) -> ConstructorKeys<'expr> {
        let db = self.db();
        if let [positional @ ast::Expr::Dict(dict)] = &*arguments.args
            && arguments.keywords.is_empty()
        {
            self.infer_typed_dict_constructor_dict_literal_values(typed_dict, dict);
            let (valid, keys) = validate_typed_dict_dict_literal(
                &self.context,
                typed_dict,
                dict,
                AnyNodeRef::from(dict),
                |expr| self.typed_dict_argument(expr),
            );
            if self.try_expression_type(positional).is_none() {
                let target = Type::TypedDict(typed_dict);
                let tcx = TypeContext::declared(Some(target));
                let ty = if valid {
                    target
                } else {
                    self.infer_dict_literal_items(dict, tcx, &FxHashMap::default())
                };
                self.finish_expression_type(positional, ty, tcx);
            }
            return keys;
        }

        self.infer_typed_dict_constructor_keyword_values(typed_dict, arguments);
        if let [_, extra, ..] = &*arguments.args {
            if let Some(builder) = self
                .context
                .report_lint(&TOO_MANY_POSITIONAL_ARGUMENTS, extra)
            {
                builder.into_diagnostic(format_args!(
                    "Too many positional arguments to TypedDict `{}` constructor: expected 1, got {}",
                    Type::TypedDict(typed_dict).display(db, self.program_environment()), arguments.args.len(),
                ));
            }
            return ConstructorKeys::new();
        }

        let keyword_keys = validate_typed_dict_constructor_keywords(
            &self.context,
            typed_dict,
            arguments,
            error_node,
            |expr| self.typed_dict_argument(expr),
        );
        if let [positional] = &*arguments.args {
            if let ast::Expr::Dict(dict) = positional {
                self.infer_typed_dict_constructor_dict_literal_values(typed_dict, dict);
                self.store_expression_type(positional, Type::unknown());
            } else if !positional.is_starred_expr() {
                let overriding_keys = keyword_keys.keys().cloned().collect();
                let target = typed_dict_with_relaxed_keys(db, typed_dict, &overriding_keys);
                self.get_or_infer_expression(
                    positional,
                    TypeContext::declared(Some(Type::TypedDict(target))),
                );
            }
        }

        validate_typed_dict_constructor(
            &self.context,
            typed_dict,
            arguments,
            error_node,
            keyword_keys,
            |expr| self.typed_dict_argument(expr),
        )
    }

    /// Infer keyword argument values for a `TypedDict` constructor.
    ///
    /// Named keywords are inferred against the declared type of the matching `TypedDict` field.
    /// Unpacked `**kwargs` and unknown keys fall back to default inference because they do not
    /// map to a single field declaration at this stage.
    fn infer_typed_dict_constructor_keyword_values(
        &mut self,
        typed_dict: TypedDictType<'db>,
        arguments: &ast::Arguments,
    ) {
        for keyword in &arguments.keywords {
            let value_tcx = keyword
                .arg
                .as_ref()
                .and_then(|arg_name| typed_dict.item(self.db(), arg_name.id.as_str()))
                .map(|field| TypeContext::declared(Some(field.declared_ty)))
                .unwrap_or_default();
            self.get_or_infer_expression(&keyword.value, value_tcx);
        }
    }

    /// Infers the entries of a positional dict literal against the constructor's field types.
    /// Validating these entries together with the keywords determines which values survive.
    fn infer_typed_dict_constructor_dict_literal_values(
        &mut self,
        typed_dict: TypedDictType<'db>,
        dict_expr: &ast::ExprDict,
    ) {
        let db = self.db();
        let env = self.program_environment();
        let key_tcx =
            TypeContext::declared(self.typed_dict_key_expected_type(Type::TypedDict(typed_dict)));

        for item in &dict_expr.items {
            let key_ty = item
                .key
                .as_ref()
                .map(|key| self.get_or_infer_expression(key, key_tcx));
            let value_tcx = if let Some(key) = key_ty.and_then(Type::as_string_literal)
                && let Some(field) = typed_dict.item(self.db(), key.value(self.db()))
            {
                TypeContext::declared(Some(field.declared_ty))
            } else if let Some(key_ty) = key_ty {
                if key_ty.is_assignable_to(db, env, KnownClass::Str.to_instance(db, env)) {
                    TypeContext::declared(typed_dict.arbitrary_key_initialization_type(db, env))
                } else {
                    TypeContext::default()
                }
            } else {
                TypeContext::default()
            };
            self.get_or_infer_expression(&item.value, value_tcx);
        }
    }

    /// Infer the `TypedDictSchema` for an "inlined"/"dangling" functional `TypedDict` definition,
    /// such as `class Foo(TypedDict("Bar", {"x": int})): ...`.
    ///
    /// Note that, as of 2026-03-29, support for these is not mandated by the spec, and they are not
    /// supported by pyrefly or zuban. However, they are supported by pyright and mypy. We also
    /// support inline schemas for `NamedTuple`s, so it makes sense to do the same for `TypedDict`s
    /// out of consistency.
    ///
    /// This method uses `self.expression_type()` for all non-type expressions: it is assumed that
    /// all non-type expressions have already been inferred by a call to `self.validate_fields_arg()`,
    /// which is called before this method in the inference process.
    fn infer_dangling_typeddict_spec(
        &mut self,
        fields_arg: &ast::Expr,
        total: bool,
    ) -> TypedDictSchema<'db> {
        let db = self.db();
        let mut schema = TypedDictSchema::default();

        let ast::Expr::Dict(dict_expr) = fields_arg else {
            return schema;
        };

        for (i, item) in dict_expr.iter().enumerate() {
            let Some(key) = &item.key else {
                for ast::DictItem { key, value } in &dict_expr.items[i + 1..] {
                    if key.is_some() {
                        self.infer_annotation_expression(value, self.deferred_state);
                    }
                }
                return TypedDictSchema::default();
            };

            let key_type = self.expression_type(key);
            let Some(key_literal) = key_type.as_string_literal() else {
                for ast::DictItem { key, value } in &dict_expr.items[i..] {
                    if key.is_some() {
                        self.infer_annotation_expression(value, self.deferred_state);
                    }
                }
                return TypedDictSchema::default();
            };

            let annotation = self.infer_typeddict_field(&item.value);

            schema.insert(
                Name::new(key_literal.value(db)),
                functional_typed_dict_field(
                    annotation.inner_type(),
                    annotation.qualifiers(),
                    total,
                ),
            );
        }

        schema
    }

    /// Infer field types for functional `TypedDict` assignments in deferred phase, for example:
    ///
    /// ```python
    /// TD = TypedDict("TD", {"x": "TD | None"}, total=False)
    /// ```
    ///
    /// This is called during `infer_deferred_types` to infer field types after the `TypedDict`
    /// definition is complete. This enables support for recursive `TypedDict`s where field types
    /// may reference the `TypedDict` being defined.
    pub(super) fn infer_functional_typeddict_deferred(&mut self, arguments: &ast::Arguments) {
        if let Some(ast::Expr::Dict(dict_expr)) = arguments.args.get(1) {
            for ast::DictItem { key, value } in dict_expr {
                if key.is_some() {
                    self.infer_typeddict_field(value);
                }
            }
        }

        if let Some(extra_items_kwarg) = arguments.find_keyword("extra_items") {
            self.infer_extra_items_kwarg(&extra_items_kwarg.value);
        }
    }

    fn infer_typeddict_field(&mut self, value: &ast::Expr) -> TypeAndQualifiers<'db> {
        let annotation = self.infer_annotation_expression(value, self.deferred_state);
        for qualifier in TypeQualifier::iter() {
            if !qualifier.is_valid_in_typeddict_field()
                && annotation
                    .qualifiers
                    .contains(TypeQualifiers::from(qualifier))
                && let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, value)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Type qualifier `{qualifier}` is not valid in a TypedDict field"
                ));
                diagnostic.info(
                    "Only `Required`, `NotRequired` and `ReadOnly` are valid in this context",
                );
            }
        }
        annotation
    }

    pub(super) fn infer_extra_items_kwarg(&mut self, value: &ast::Expr) -> TypeAndQualifiers<'db> {
        let annotation = self.infer_annotation_expression(value, self.deferred_state);
        for qualifier in TypeQualifier::iter() {
            if qualifier != TypeQualifier::ReadOnly
                && annotation
                    .qualifiers
                    .contains(TypeQualifiers::from(qualifier))
                && let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, value)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Type qualifier `{qualifier}` is not valid in a TypedDict `extra_items` argument"
                ));
                diagnostic.info("`ReadOnly` is the only permitted type qualifier here");
            }
        }
        annotation
    }

    /// Infer all non-type expressions in the `fields` argument of a functional `TypedDict` definition,
    /// and emit diagnostics for invalid field keys. Type expressions are not inferred during this pass,
    /// because it must be deferred for` TypedDict` definitions that may hold recursive references to
    /// themselves.
    fn validate_fields_arg(&mut self, fields_arg: &ast::Expr) {
        let db = self.db();
        if let ast::Expr::Dict(dict_expr) = fields_arg {
            for ast::DictItem { key, value } in dict_expr {
                if let Some(key) = key {
                    let key_type = self.infer_expression(key, TypeContext::default());
                    if !key_type.is_string_literal()
                        && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, key)
                    {
                        let mut diagnostic = builder.into_diagnostic(
                            "Expected a string-literal key \
                                in the `fields` dict of `TypedDict()`",
                        );
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Found `{}`",
                            key_type.display(db, self.program_environment())
                        ));
                    }
                } else {
                    self.infer_expression(value, TypeContext::default());
                    if let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, value) {
                        builder.into_diagnostic(
                            "Keyword splats are not allowed in the `fields` \
                            parameter to `TypedDict()`",
                        );
                    }
                }
            }
        } else {
            self.infer_expression(fields_arg, TypeContext::default());

            if let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, fields_arg) {
                builder.into_diagnostic(
                    "Expected a dict literal for parameter `fields` of `TypedDict()`",
                );
            }
        }
    }
}
