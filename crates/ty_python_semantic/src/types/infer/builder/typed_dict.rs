use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, NodeIndex};

use super::TypeInferenceBuilder;
use crate::semantic_index::definition::Definition;
use crate::types::class::{ClassLiteral, DynamicTypedDictAnchor, DynamicTypedDictLiteral};
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, MISSING_ARGUMENT, TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
};
use crate::types::typed_dict::{
    FunctionalTypedDictSpec, TypedDictSchema, functional_typed_dict_field,
};
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

        let fields_type = fields_arg.and_then(|fields_arg| {
            if matches!(fields_arg, ast::Expr::Dict(_)) {
                // the `fields` arg contains annotation expressions,
                // so inference is deferred until a later stage
                None
            } else {
                Some(self.infer_expression(fields_arg, TypeContext::default()))
            }
        });

        for arg in args.iter().skip(2) {
            self.infer_expression(arg, TypeContext::default());
        }

        if has_starred || has_double_starred {
            for kw in keywords {
                self.infer_expression(&kw.value, TypeContext::default());
                if let Some(arg) = &kw.arg {
                    if !matches!(arg.id.as_str(), "total" | "closed" | "extra_items")
                        && let Some(builder) = self.context.report_lint(&UNKNOWN_ARGUMENT, kw)
                    {
                        builder.into_diagnostic(format_args!(
                            "Argument `{}` does not match any known parameter of function `TypedDict`",
                            arg.id
                        ));
                    }
                }
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
                field_name => {
                    self.infer_expression(&kw.value, TypeContext::default());
                    if let Some(builder) = self.context.report_lint(&UNKNOWN_ARGUMENT, kw) {
                        builder.into_diagnostic(format_args!(
                            "Argument `{field_name}` does not match any known parameter of function `TypedDict`",
                        ));
                    }
                }
            }
        }

        if fields_arg.is_none()
            && let Some(builder) = self.context.report_lint(&MISSING_ARGUMENT, call_expr)
        {
            builder.into_diagnostic(
                "No argument provided for required parameter `fields` of function `TypedDict`",
            );
        }

        let name = if let Some(literal) = name_type.as_string_literal() {
            Name::new(literal.value(db))
        } else {
            if !name_type.is_assignable_to(db, KnownClass::Str.to_instance(db))
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Invalid argument to parameter `typename` of `TypedDict()`"
                ));
                diagnostic.set_primary_message(format_args!(
                    "Expected `str`, found `{}`",
                    name_type.display(db)
                ));
            }
            Name::new_static("<unknown>")
        };

        if let Some(definition) = definition {
            self.deferred.insert(definition);
        }

        let fields_are_known = fields_arg
            .map(|fields_arg| self.typeddict_fields_are_known(fields_arg, fields_type))
            .unwrap_or(true);

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

                let spec = if let Some(fields_arg) = fields_arg {
                    if fields_are_known {
                        self.infer_dangling_typeddict_spec(fields_arg, total)
                    } else {
                        self.infer_typeddict_field_types(fields_arg);
                        FunctionalTypedDictSpec::unknown(db)
                    }
                } else {
                    FunctionalTypedDictSpec::known(db, TypedDictSchema::default())
                };

                DynamicTypedDictAnchor::ScopeOffset {
                    scope,
                    offset: call_u32 - anchor_u32,
                    spec,
                }
            }
        };

        let typeddict = DynamicTypedDictLiteral::new(db, name, anchor);

        Type::ClassLiteral(ClassLiteral::DynamicTypedDict(typeddict))
    }

    fn infer_dangling_typeddict_spec(
        &mut self,
        fields_arg: &ast::Expr,
        total: bool,
    ) -> FunctionalTypedDictSpec<'db> {
        let db = self.db();
        let mut schema = TypedDictSchema::default();

        let ast::Expr::Dict(dict_expr) = fields_arg else {
            return FunctionalTypedDictSpec::unknown(db);
        };

        for item in &dict_expr.items {
            let Some(key) = &item.key else {
                return FunctionalTypedDictSpec::unknown(db);
            };

            let key_ty = self
                .try_expression_type(key)
                .unwrap_or_else(|| self.infer_expression(key, TypeContext::default()));
            let Some(key_literal) = key_ty.as_string_literal() else {
                return FunctionalTypedDictSpec::unknown(db);
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

        FunctionalTypedDictSpec::known(db, schema)
    }

    /// Infer field types for functional `TypedDict` in deferred phase.
    ///
    /// This is called during `infer_deferred_types` to infer field types after the `TypedDict`
    /// definition is complete. This enables support for recursive `TypedDict`s where field types
    /// may reference the `TypedDict` being defined.
    pub(super) fn infer_functional_typeddict_deferred(&mut self, arguments: &ast::Arguments) {
        if let Some(fields_arg) = arguments.args.get(1) {
            self.infer_typeddict_field_types(fields_arg);
        }

        for kw in &arguments.keywords {
            if let Some(arg) = &kw.arg {
                match arg.id.as_str() {
                    "total" | "closed" => continue,
                    "extra_items" => {
                        self.infer_annotation_expression(&kw.value, self.deferred_state);
                    }
                    _ => {
                        self.infer_expression(&kw.value, TypeContext::default());
                    }
                }
            }
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

    fn typeddict_fields_are_known(
        &mut self,
        fields_arg: &ast::Expr,
        fields_type: Option<Type<'db>>,
    ) -> bool {
        let db = self.db();

        if let ast::Expr::Dict(dict_expr) = fields_arg {
            for item in &dict_expr.items {
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
                    return false;
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
                    return false;
                }
            }

            return true;
        }

        if let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, fields_arg) {
            if let Some(fields_type) = fields_type {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Expected a dict literal for parameter `fields` of `TypedDict()`"
                ));
                diagnostic.set_primary_message(format_args!("Found `{}`", fields_type.display(db)));
            } else {
                builder.into_diagnostic(
                    "Expected a dict literal for parameter `fields` of `TypedDict()`",
                );
            }
        }

        false
    }
}
