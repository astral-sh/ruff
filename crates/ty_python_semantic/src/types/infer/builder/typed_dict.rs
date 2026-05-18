use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, AnyNodeRef, HasNodeIndex, NodeIndex};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use strum::IntoEnumIterator;

use super::TypeInferenceBuilder;
use crate::TypeQualifiers;
use crate::types::class::{ClassLiteral, DynamicTypedDictAnchor, DynamicTypedDictLiteral};
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, INVALID_TYPE_FORM, MISSING_ARGUMENT, TOO_MANY_POSITIONAL_ARGUMENTS,
    UNKNOWN_ARGUMENT, report_mismatched_type_name,
};
use crate::types::infer::builder::DeferredExpressionState;
use crate::types::special_form::TypeQualifier;
use crate::types::typed_dict::{
    TypedDictSchema, collect_guaranteed_keyword_keys, functional_typed_dict_field,
    infer_unpacked_keyword_types, typed_dict_with_relaxed_keys, validate_typed_dict_constructor,
    validate_typed_dict_dict_literal,
};
use crate::types::{
    IntersectionType, KnownClass, Type, TypeAndQualifiers, TypeContext, TypedDictType,
};
use ty_python_core::definition::Definition;

/// The shape of a `TypedDict` constructor call that affects how we prepare it for inference.
#[derive(Debug, Clone, Copy)]
pub(super) enum TypedDictConstructorForm<'expr> {
    /// // Ex) `TD(x=1)`
    KeywordOnly,
    /// // Ex) `TD({"x": 1})`
    LiteralOnly(&'expr ast::Expr),
    /// // Ex) `TD(other)`
    SinglePositional(&'expr ast::Expr),
    /// // Ex) `TD({"x": 1}, y=2)`
    MixedLiteralAndKeywords(&'expr ast::ExprDict),
    /// // Ex) `TD(other, y=2)`
    MixedPositionalAndKeywords,
    /// // Ex) `TD(*args)` or `TD(*args, y=2)`
    VariadicPositional,
    /// // Ex) `TD(arg1, arg2)`
    MultiplePositionalArguments,
}

impl<'expr> TypedDictConstructorForm<'expr> {
    /// Return the constructor form for `arguments`.
    pub(super) fn from_arguments(arguments: &'expr ast::Arguments) -> Self {
        let [argument] = &arguments.args[..] else {
            return if arguments.args.is_empty() {
                Self::KeywordOnly
            } else {
                Self::MultiplePositionalArguments
            };
        };

        match (argument, arguments.keywords.is_empty()) {
            (argument, _) if argument.is_starred_expr() => Self::VariadicPositional,
            (ast::Expr::Dict(_), true) => Self::LiteralOnly(argument),
            (ast::Expr::Dict(dict_expr), false) => Self::MixedLiteralAndKeywords(dict_expr),
            (_, true) => Self::SinglePositional(argument),
            (_, false) => Self::MixedPositionalAndKeywords,
        }
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer a `TypedDict(name, fields)` call expression.
    ///
    /// This method *does not* call `infer_expression` on the object being called;
    /// it is assumed that the type for this AST node has already been inferred before this method is called.
    pub(super) fn infer_typeddict_call_expression(
        &mut self,
        call_expr: &ast::ExprCall,
        definition: Option<Definition<'db>>,
    ) -> Type<'db> {
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
            let spec = &[KnownClass::Str.to_instance(db), Type::object()];
            let str_object_map = KnownClass::Mapping.to_specialized_subclass_of(db, spec);
            IntersectionType::from_two_elements(db, str_object_map, Type::unknown())
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

        for kw in keywords {
            let Some(arg) = &kw.arg else {
                continue;
            };

            match &**arg {
                arg_name @ ("total" | "closed") => {
                    let kw_type = self.infer_expression(&kw.value, TypeContext::default());
                    if kw_type
                        .as_literal_value()
                        .is_none_or(|literal| !literal.is_bool())
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, &kw.value)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Invalid argument to parameter `{arg_name}` of `TypedDict()`"
                        ));
                        diagnostic.set_primary_message(format_args!(
                            "Expected either `True` or `False`, got object of type `{}`",
                            kw_type.display(db)
                        ));
                    }

                    if arg_name == "total" {
                        if kw_type.bool(db).is_always_false() {
                            total = false;
                        } else if !kw_type.bool(db).is_always_true() {
                            total = true;
                        }
                    }
                }
                "extra_items" => {
                    if definition.is_none() {
                        self.infer_extra_items_kwarg(&kw.value);
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
            .map(|literal| Name::new(literal.value(db)));

        if name.is_none()
            && !name_type.is_assignable_to(db, KnownClass::Str.to_instance(db))
            && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
        {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Invalid argument to parameter `typename` of `TypedDict()`"
            ));
            diagnostic.set_primary_message(format_args!(
                "Expected `str`, found `{}`",
                name_type.display(db)
            ));
        } else if let Some(definition) = definition
            && let Some(assigned_name) = definition.name(db)
            && Some(assigned_name.as_str()) != name.as_deref()
        {
            report_mismatched_type_name(
                &self.context,
                name_arg,
                "TypedDict",
                &assigned_name,
                name.as_deref(),
                name_type,
            );
        }

        let name = name.unwrap_or_else(|| Name::new_static("<unknown>"));

        self.validate_fields_arg(fields_arg);

        if let Some(definition) = definition {
            self.deferred.insert(definition);
        }

        let scope = self.scope();
        let anchor = match definition {
            Some(definition) => DynamicTypedDictAnchor::Definition(definition),
            None => {
                let call_node_index = call_expr.node_index.load();
                let scope_anchor = scope.node(db).node_index().unwrap_or(NodeIndex::from(0));
                let anchor_u32 = scope_anchor
                    .as_u32()
                    .expect("scope anchor should not be NodeIndex::NONE");
                let call_u32 = call_node_index
                    .as_u32()
                    .expect("call node should not be NodeIndex::NONE");
                let schema = self.infer_dangling_typeddict_spec(fields_arg, total);

                DynamicTypedDictAnchor::ScopeOffset {
                    scope,
                    offset: call_u32 - anchor_u32,
                    schema,
                }
            }
        };

        let typeddict = DynamicTypedDictLiteral::new(db, name, anchor);
        Type::ClassLiteral(ClassLiteral::DynamicTypedDict(typeddict))
    }

    pub(super) fn infer_typed_dict_expression(
        &mut self,
        dict: &ast::ExprDict,
        typed_dict: TypedDictType<'db>,
        item_types: &mut FxHashMap<NodeIndex, Type<'db>>,
    ) -> Option<Type<'db>> {
        let ast::ExprDict {
            range: _,
            node_index: _,
            items,
        } = dict;

        let typed_dict_items = typed_dict.items(self.db());
        let key_tcx =
            TypeContext::new(self.typed_dict_key_expected_type(Type::TypedDict(typed_dict)));

        for item in items {
            let key_ty = self.infer_optional_expression(item.key.as_ref(), key_tcx);
            if let Some((key, key_ty)) = item.key.as_ref().zip(key_ty) {
                item_types.insert(key.node_index().load(), key_ty);
            }

            let value_ty = if let Some(key_ty) = key_ty
                && let Some(key) = key_ty.as_string_literal()
                && let Some(field) = typed_dict_items.get(key.value(self.db()))
            {
                self.infer_expression(&item.value, TypeContext::new(Some(field.declared_ty)))
            } else {
                self.infer_expression(&item.value, TypeContext::default())
            };

            item_types.insert(item.value.node_index().load(), value_ty);
        }

        validate_typed_dict_dict_literal(
            &self.context,
            typed_dict,
            dict,
            dict.into(),
            |expr: &ast::Expr, tcx: TypeContext<'db>| {
                item_types
                    .get(&expr.node_index().load())
                    .copied()
                    .unwrap_or_else(|| {
                        let _ = tcx;
                        Type::unknown()
                    })
            },
        )
        .ok()
        .map(|_| Type::TypedDict(typed_dict))
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
        form: TypedDictConstructorForm<'expr>,
        arguments: &'expr ast::Arguments,
        error_node: AnyNodeRef<'expr>,
    ) {
        match form {
            TypedDictConstructorForm::LiteralOnly(argument) => {
                let target_ty = Type::TypedDict(typed_dict);
                self.get_or_infer_expression(argument, TypeContext::new(Some(target_ty)));
                return;
            }
            TypedDictConstructorForm::SinglePositional(argument) => {
                let target_ty = Type::TypedDict(typed_dict);
                self.get_or_infer_expression(argument, TypeContext::new(Some(target_ty)));
            }
            TypedDictConstructorForm::MixedPositionalAndKeywords => {
                let unpacked_keyword_types =
                    infer_unpacked_keyword_types(arguments, |expr, tcx| {
                        self.get_or_infer_expression(expr, tcx)
                    });
                let keyword_keys = collect_guaranteed_keyword_keys(
                    self.db(),
                    typed_dict,
                    arguments,
                    &unpacked_keyword_types,
                    &mut |expr, tcx| self.get_or_infer_expression(expr, tcx),
                );
                let positional_target =
                    typed_dict_with_relaxed_keys(self.db(), typed_dict, &keyword_keys);
                let target_ty = Type::TypedDict(positional_target);
                self.get_or_infer_expression(&arguments.args[0], TypeContext::new(Some(target_ty)));
            }
            TypedDictConstructorForm::MixedLiteralAndKeywords(dict_expr) => {
                self.infer_typed_dict_constructor_dict_literal_values(typed_dict, dict_expr);
                self.store_expression_type(&arguments.args[0], Type::unknown());
            }
            TypedDictConstructorForm::KeywordOnly
            | TypedDictConstructorForm::VariadicPositional
            | TypedDictConstructorForm::MultiplePositionalArguments => {}
        }

        if !arguments.keywords.is_empty() {
            self.infer_typed_dict_constructor_keyword_values(typed_dict, arguments);
        }

        validate_typed_dict_constructor(
            &self.context,
            typed_dict,
            arguments,
            error_node,
            |expr, _| self.expression_type(expr),
        );
    }

    /// Infer keyword argument values for a `TypedDict` constructor.
    ///
    /// Named keywords are inferred against the declared type of the matching `TypedDict` field.
    /// Unpacked `**kwargs` and unknown keys fall back to default inference because they do not
    /// map to a single field declaration at this stage.
    pub(super) fn infer_typed_dict_constructor_keyword_values(
        &mut self,
        typed_dict: TypedDictType<'db>,
        arguments: &ast::Arguments,
    ) {
        let items = typed_dict.items(self.db());
        for keyword in &arguments.keywords {
            let value_tcx = keyword
                .arg
                .as_ref()
                .and_then(|arg_name| items.get(arg_name.id.as_str()))
                .map(|field| TypeContext::new(Some(field.declared_ty)))
                .unwrap_or_default();
            self.get_or_infer_expression(&keyword.value, value_tcx);
        }
    }

    /// Infer the key and value expressions of a positional dict literal passed to a
    /// `TypedDict` constructor alongside keyword arguments.
    ///
    /// The outer dict literal is intentionally left uninferred for later call binding; this helper only
    /// pre-infers its nested expressions so full constructor validation can still combine keys
    /// from the dict literal and keyword arguments without double-inferring the dict itself.
    fn infer_typed_dict_constructor_dict_literal_values(
        &mut self,
        typed_dict: TypedDictType<'db>,
        dict_expr: &ast::ExprDict,
    ) {
        let items = typed_dict.items(self.db());
        let key_tcx =
            TypeContext::new(self.typed_dict_key_expected_type(Type::TypedDict(typed_dict)));

        for item in &dict_expr.items {
            let value_tcx = item
                .key
                .as_ref()
                .map(|key| self.get_or_infer_expression(key, key_tcx))
                .and_then(Type::as_string_literal)
                .and_then(|key| items.get(key.value(self.db())))
                .map(|field| TypeContext::new(Some(field.declared_ty)))
                .unwrap_or_default();
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
        let state = if self.in_stub() {
            DeferredExpressionState::Deferred
        } else {
            self.deferred_state
        };
        let annotation = self.infer_annotation_expression(value, state);
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
                        diagnostic
                            .set_primary_message(format_args!("Found `{}`", key_type.display(db)));
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
