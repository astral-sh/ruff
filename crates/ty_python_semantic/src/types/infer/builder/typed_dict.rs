use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, NodeIndex};

use super::TypeInferenceBuilder;
use crate::semantic_index::definition::Definition;
use crate::types::class::{ClassLiteral, DynamicTypedDictAnchor, DynamicTypedDictLiteral};
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, MISSING_ARGUMENT, TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
};
use crate::types::typed_dict::{TypedDictSchema, functional_typed_dict_field};
use crate::types::{IntersectionType, KnownClass, Type, TypeContext};

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

        let has_starred = args.iter().any(ast::Expr::is_starred_expr);
        let has_double_starred = keywords.iter().any(|kw| kw.arg.is_none());

        // The fallback type reflects the fact that if the call were successful,
        // it would return a class that is a subclass of `Mapping[str, object]`
        // with an unknown set of fields.
        let fallback = || {
            let spec = &[KnownClass::Str.to_instance(db), Type::object()];
            let str_object_map = KnownClass::Mapping.to_specialized_subclass_of(db, spec);
            IntersectionType::from_two_elements(db, str_object_map, Type::unknown())
        };

        // Emit diagnostic for unsupported variadic arguments.
        if (has_starred || has_double_starred)
            && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, call_expr)
        {
            let arg_type = if has_starred && has_double_starred {
                "Variadic positional and keyword arguments are"
            } else if has_starred {
                "Variadic positional arguments are"
            } else {
                "Variadic keyword arguments are"
            };
            builder.into_diagnostic(format_args!(
                "{arg_type} not supported in `TypedDict()` calls"
            ));
        }

        let Some(name_arg) = args.first() else {
            for arg in args {
                self.infer_expression(arg, TypeContext::default());
            }
            for kw in keywords {
                self.infer_expression(&kw.value, TypeContext::default());
            }

            if !has_starred
                && !has_double_starred
                && let Some(builder) = self.context.report_lint(&MISSING_ARGUMENT, call_expr)
            {
                builder.into_diagnostic(
                    "No argument provided for required parameter `typename` of function `TypedDict`",
                );
            }

            return fallback();
        };

        let name_type = self.infer_expression(name_arg, TypeContext::default());
        let fields_arg = args.get(1);

        for arg in args.iter().skip(2) {
            self.infer_expression(arg, TypeContext::default());
        }

        if args.len() > 2
            && !has_starred
            && !has_double_starred
            && let Some(builder) = self
                .context
                .report_lint(&TOO_MANY_POSITIONAL_ARGUMENTS, &args[2])
        {
            builder.into_diagnostic(format_args!(
                "Too many positional arguments to function `TypedDict`: expected 2, got {}",
                args.len()
            ));
        }

        let mut total = true;

        for kw in keywords {
            let Some(arg) = &kw.arg else {
                continue;
            };

            match arg.id.as_str() {
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
                        self.infer_annotation_expression(&kw.value, self.deferred_state);
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

        if has_double_starred || has_starred {
            return fallback();
        }

        if fields_arg.is_none()
            && let Some(builder) = self.context.report_lint(&MISSING_ARGUMENT, call_expr)
        {
            builder.into_diagnostic(
                "No argument provided for required parameter `fields` of function `TypedDict`",
            );
        }

        let name = if let Some(literal) = name_type.as_string_literal() {
            let name = literal.value(db);

            if let Some(assigned_name) = definition.and_then(|definition| definition.name(db))
                && name != assigned_name
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
            {
                builder.into_diagnostic(format_args!(
                    "The name of a `TypedDict` (`{name}`) must match \
                    the name of the variable it is assigned to (`{assigned_name}`)"
                ));
            }

            Name::new(name)
        } else {
            let is_str = name_type.is_assignable_to(db, KnownClass::Str.to_instance(db));
            if let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg) {
                if let Some(assigned_name) = definition.and_then(|definition| definition.name(db))
                    && is_str
                {
                    builder.into_diagnostic(format_args!(
                        "The first argument to `TypedDict` must be the string literal `{assigned_name}`"
                    ));
                } else if is_str {
                    builder.into_diagnostic(
                        "The first argument to `TypedDict` must be a string literal",
                    );
                } else {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Invalid argument to parameter `typename` of `TypedDict()`"
                    ));
                    diagnostic.set_primary_message(format_args!(
                        "Expected `str`, found `{}`",
                        name_type.display(db)
                    ));
                }
            }
            Name::new_static("<unknown>")
        };

        if let Some(definition) = definition {
            self.deferred.insert(definition);
        }

        if let Some(fields_arg) = fields_arg {
            self.validate_fields_arg(fields_arg);
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

                let schema = if let Some(fields_arg) = fields_arg {
                    self.infer_dangling_typeddict_spec(fields_arg, total)
                } else {
                    TypedDictSchema::default()
                };

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

        for item in &dict_expr.items {
            let Some(key) = &item.key else {
                return TypedDictSchema::default();
            };

            let key_ty = self.expression_type(key);
            let Some(key_literal) = key_ty.as_string_literal() else {
                return TypedDictSchema::default();
            };

            let annotation = self.infer_annotation_expression(&item.value, self.deferred_state);

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
        if let Some(fields_arg) = arguments.args.get(1) {
            self.infer_typeddict_field_types(fields_arg);
        }

        if let Some(extra_items_kwarg) = arguments.find_keyword("extra_items") {
            self.infer_annotation_expression(&extra_items_kwarg.value, self.deferred_state);
        }
    }

    /// Infer field types from a `TypedDict` fields dict argument.
    fn infer_typeddict_field_types(&mut self, fields_arg: &ast::Expr) {
        if let ast::Expr::Dict(dict_expr) = fields_arg {
            for item in &dict_expr.items {
                self.infer_annotation_expression(&item.value, self.deferred_state);
            }
        }
    }

    /// Infer all non-type expressions in the `fields` argument of a functional `TypedDict` definition,
    /// and emit diagnostics for invalid field keys. Type expressions are not inferred during this pass,
    /// because it must be deferred for` TypedDict` definitions that may hold recursive references to
    /// themselves.
    fn validate_fields_arg(&mut self, fields_arg: &ast::Expr) {
        let db = self.db();

        if let ast::Expr::Dict(dict_expr) = fields_arg {
            for (i, item) in dict_expr.items.iter().enumerate() {
                let ast::DictItem { key, value: _ } = item;

                let Some(key) = key else {
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_ARGUMENT_TYPE, fields_arg)
                    {
                        builder.into_diagnostic(
                            "Expected a dict literal with string-literal keys \
                                for parameter `fields` of `TypedDict()`",
                        );
                    }
                    for item in &dict_expr.items[i + 1..] {
                        if let Some(key) = &item.key {
                            self.infer_expression(key, TypeContext::default());
                        }
                    }
                    return;
                };

                let key_ty = self.infer_expression(key, TypeContext::default());
                if key_ty.as_string_literal().is_none() {
                    if let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, key) {
                        let mut diagnostic = builder.into_diagnostic(
                            "Expected a string-literal key \
                                in the `fields` dict of `TypedDict()`",
                        );
                        diagnostic
                            .set_primary_message(format_args!("Found `{}`", key_ty.display(db)));
                    }
                    for item in &dict_expr.items[i + 1..] {
                        if let Some(key) = &item.key {
                            self.infer_expression(key, TypeContext::default());
                        }
                    }
                    return;
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
