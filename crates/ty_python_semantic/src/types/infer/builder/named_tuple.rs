use std::borrow::Cow;

use crate::{
    Db,
    types::{
        ClassLiteral, IntersectionType, KnownClass, KnownInstanceType, SpecialFormType, Type,
        TypeContext, UnionType,
        class::{
            DynamicNamedTupleAnchor, DynamicNamedTupleLiteral, FunctionalNamedTupleField,
            NamedTupleField, NamedTupleSpec,
        },
        diagnostic::{
            INVALID_ARGUMENT_TYPE, INVALID_NAMED_TUPLE, MISSING_ARGUMENT,
            NON_LITERAL_NAMED_TUPLE_FIELDS, PARAMETER_ALREADY_ASSIGNED,
            TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT, report_mismatched_type_name,
        },
        extract_fixed_length_iterable_element_types,
        function::KnownFunction,
        infer::TypeInferenceBuilder,
        object_type_form,
        tuple::{TupleLength, TupleSpec},
        unpacker::sequence_elts,
    },
};
use char_str::format_char;
use ruff_python_ast::{self as ast, name::Name};
use ruff_python_stdlib::{identifiers::is_identifier, keyword::is_keyword};
use rustc_hash::FxHashSet;
use ty_python_core::definition::Definition;

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer a `typing.NamedTuple(typename, fields)` or `collections.namedtuple(typename, field_names)` call.
    ///
    /// This method *does not* call `infer_expression` on the object being called;
    /// it is assumed that the type for this AST node has already been inferred before this method is called.
    pub(super) fn infer_namedtuple_call_expression(
        &mut self,
        call_expr: &ast::ExprCall,
        definition: Option<Definition<'db>>,
        kind: NamedTupleKind,
    ) -> Type<'db> {
        let env = self.program_environment();
        let db = self.db();

        // The fallback type reflects the fact that if the call were successful,
        // it would return a class that:
        //
        // - Would be a subclass of `tuple[Unknown, ...]`
        // - Would have all the generated methods included on the `NamedTupleLike` protocol
        // - Would have a constructor method that would accept an unknown set of positional
        //   and keyword arguments
        let fallback = || {
            IntersectionType::from_elements(
                db,
                env,
                [
                    Type::homogeneous_tuple(db, env, Type::unknown()).to_meta_type(db, env),
                    KnownClass::NamedTupleLike.to_subclass_of(db, env),
                    Type::unknown(),
                ],
            )
        };

        let ast::Arguments {
            args,
            keywords,
            range: _,
            node_index: _,
        } = &call_expr.arguments;

        // Check for variadic arguments early, before extracting positional args.
        let has_starred = args.iter().any(ast::Expr::is_starred_expr);
        let has_double_starred = keywords.iter().any(|kw| kw.arg.is_none());

        // Emit diagnostic for missing required arguments or unsupported variadic arguments.
        // For `typing.NamedTuple`, emit a diagnostic since variadic arguments are not supported.
        // For `collections.namedtuple`, silently fall back since it's more permissive at runtime.
        if (has_starred || has_double_starred)
            && kind.is_typing()
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
                "{arg_type} not supported in `NamedTuple()` calls"
            ));
        }

        // Extract typename and fields from positional or keyword arguments.
        // For `collections.namedtuple`, both `typename` and `field_names` can be keyword arguments.
        // For `typing.NamedTuple`, only positional arguments are supported.
        let (name_arg, fields_arg, rest, name_from_keyword, fields_from_keyword): (
            Option<&ast::Expr>,
            Option<&ast::Expr>,
            &[ast::Expr],
            bool,
            bool,
        ) = match kind {
            NamedTupleKind::Collections => {
                let typename_kw = call_expr.arguments.find_keyword("typename");
                let field_names_kw = call_expr.arguments.find_keyword("field_names");

                match &**args {
                    [name, fields, rest @ ..] => (Some(name), Some(fields), rest, false, false),
                    [name, rest @ ..] => (
                        Some(name),
                        field_names_kw.map(|kw| &kw.value),
                        rest,
                        false,
                        field_names_kw.is_some(),
                    ),
                    [] => (
                        typename_kw.map(|kw| &kw.value),
                        field_names_kw.map(|kw| &kw.value),
                        &[],
                        typename_kw.is_some(),
                        field_names_kw.is_some(),
                    ),
                }
            }
            NamedTupleKind::Typing => match &**args {
                [name, fields, rest @ ..] => (Some(name), Some(fields), rest, false, false),
                [name, rest @ ..] => (Some(name), None, rest, false, false),
                [] => (None, None, &[], false, false),
            },
        };

        // Check if we have both required arguments.
        let (Some(name_arg), Some(fields_arg)) = (name_arg, fields_arg) else {
            for arg in args {
                self.infer_expression(arg, TypeContext::default());
            }
            for kw in keywords {
                self.infer_expression(&kw.value, TypeContext::default());
            }

            if !has_starred && !has_double_starred {
                let fields_param_name = match kind {
                    NamedTupleKind::Typing => "fields",
                    NamedTupleKind::Collections => "field_names",
                };
                let missing = match (name_arg.is_none(), fields_arg.is_none()) {
                    (true, true) => format!("`typename` and `{fields_param_name}`"),
                    (true, false) => "`typename`".to_string(),
                    (false, true) => format!("`{fields_param_name}`"),
                    (false, false) => unreachable!(),
                };
                let plural = name_arg.is_none() && fields_arg.is_none();
                if let Some(builder) = self.context.report_lint(&MISSING_ARGUMENT, call_expr) {
                    builder.into_diagnostic(format_args!(
                        "Missing required argument{} {missing} to `{kind}()`",
                        if plural { "s" } else { "" }
                    ));
                }
            }
            return fallback();
        };

        let name_type = self.infer_expression(name_arg, TypeContext::default());

        for arg in rest {
            self.infer_expression(arg, TypeContext::default());
        }

        // If any argument is a starred expression or any keyword is a double-starred expression,
        // we can't statically determine the arguments, so fall back to normal call binding.
        if has_starred || has_double_starred {
            self.infer_expression(fields_arg, TypeContext::default());

            for kw in keywords {
                if let Some(arg) = kw.arg.as_deref() {
                    if name_from_keyword && arg == "typename" {
                        continue;
                    }
                    if fields_from_keyword && arg == "field_names" {
                        continue;
                    }
                }
                self.infer_expression(&kw.value, TypeContext::default());
            }
            return fallback();
        }

        // Check for excess positional arguments (only `typename` and `fields` are expected).
        if !rest.is_empty() {
            if let Some(builder) = self
                .context
                .report_lint(&TOO_MANY_POSITIONAL_ARGUMENTS, &rest[0])
            {
                builder.into_diagnostic(format_args!(
                    "Too many positional arguments to function `{kind}`: expected 2, got {}",
                    args.len()
                ));
            }
        }

        // Infer keyword arguments.
        let mut default_types: Vec<Type<'db>> = vec![];
        let mut defaults_kw: Option<&ast::Keyword> = None;
        let mut rename_type = None;

        for kw in keywords {
            // `kw.arg` is `None` for double-starred kwargs (`**kwargs`), but we already
            // returned early above if there were any, so this should always be `Some`.
            let arg = kw
                .arg
                .as_ref()
                .expect("double-starred kwargs should have been handled above");

            // Skip keywords that were used for the required arguments (already inferred above).
            // These flags are only true for `collections.namedtuple`.
            if name_from_keyword && arg.id.as_str() == "typename" {
                continue;
            }
            if fields_from_keyword && arg.id.as_str() == "field_names" {
                continue;
            }

            let kw_type = self.infer_expression(&kw.value, TypeContext::default());

            match arg.id.as_str() {
                "defaults" if kind.is_collections() => {
                    defaults_kw = Some(kw);
                    if let Some(element_types) =
                        extract_fixed_length_iterable_element_types(db, env, &kw.value, |expr| {
                            self.expression_type(expr)
                        })
                    {
                        default_types = element_types.into_vec();
                    } else {
                        // Can't determine individual types; use Any for each element.
                        let count = kw_type
                            .exact_tuple_instance_spec(db)
                            .and_then(|spec| spec.len().maximum())
                            .unwrap_or(0);
                        default_types = vec![Type::any(); count];
                    }
                    // Emit diagnostic for invalid types (not Iterable[Any] | None).
                    let iterable_any =
                        KnownClass::Iterable.to_specialized_instance(db, env, &[Type::any()]);
                    let valid_type =
                        UnionType::from_two_elements(db, env, iterable_any, Type::none(db, env));
                    if !kw_type.is_assignable_to(db, env, valid_type)
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, &kw.value)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Invalid argument to parameter `defaults` of `namedtuple()`"
                        ));
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Expected `Iterable[Any] | None`, found `{}`",
                            kw_type.display(db, env)
                        ));
                    }
                }
                "rename" if kind.is_collections() => {
                    rename_type = Some(kw_type);

                    // Emit diagnostic for non-bool types.
                    if !kw_type.is_assignable_to(db, env, KnownClass::Bool.to_instance(db, env))
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, &kw.value)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Invalid argument to parameter `rename` of `namedtuple()`"
                        ));
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Expected `bool`, found `{}`",
                            kw_type.display(db, env)
                        ));
                    }
                }
                "module" if kind.is_collections() => {
                    // Emit diagnostic for invalid types (not str | None).
                    let valid_type = UnionType::from_two_elements(
                        db,
                        env,
                        KnownClass::Str.to_instance(db, env),
                        Type::none(db, env),
                    );
                    if !kw_type.is_assignable_to(db, env, valid_type)
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, &kw.value)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Invalid argument to parameter `module` of `namedtuple()`"
                        ));
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Expected `str | None`, found `{}`",
                            kw_type.display(db, env)
                        ));
                    }
                }
                // `typename` is valid as a keyword argument only for `collections.namedtuple`.
                // If it was already provided positionally, emit an error.
                "typename" if kind.is_collections() => {
                    if !args.is_empty() {
                        if let Some(builder) =
                            self.context.report_lint(&PARAMETER_ALREADY_ASSIGNED, kw)
                        {
                            builder.into_diagnostic(format_args!(
                                "Multiple values provided for parameter `typename` of `{kind}`"
                            ));
                        }
                    }
                }
                // `field_names` is valid only for `collections.namedtuple`.
                // If it was already provided positionally, emit an error.
                "field_names" if kind.is_collections() => {
                    if args.len() >= 2 {
                        if let Some(builder) =
                            self.context.report_lint(&PARAMETER_ALREADY_ASSIGNED, kw)
                        {
                            builder.into_diagnostic(format_args!(
                                "Multiple values provided for parameter `field_names` of `{kind}`"
                            ));
                        }
                    }
                }
                unknown_kwarg => {
                    // Report unknown keyword argument.
                    if let Some(builder) = self.context.report_lint(&UNKNOWN_ARGUMENT, kw) {
                        builder.into_diagnostic(format_args!(
                            "Argument `{unknown_kwarg}` does not match any known parameter of function `{kind}`",
                        ));
                    }
                }
            }
        }

        // Extract name.
        let name = name_type
            .as_string_literal()
            .map(|literal| literal.value(db));

        if name.is_none()
            && !name_type.is_assignable_to(db, env, KnownClass::Str.to_instance(db, env))
            && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
        {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Invalid argument to parameter `typename` of `{kind}()`"
            ));
            diagnostic.set_primary_annotation_message(format_args!(
                "Expected `str`, found `{}`",
                name_type.display(db, env)
            ));
        } else if let Some(actual_name) = name
            && let Some(definition) = definition
            && let Some(assigned_name) = definition.name(db)
            && assigned_name.as_str() != actual_name
        {
            report_mismatched_type_name(
                &self.context,
                name_arg,
                &kind.to_string(),
                &assigned_name,
                Some(actual_name),
                name_type,
            );
        }

        let name = name.unwrap_or("<unknown>");

        // Handle fields based on which namedtuple variant.
        let anchor = match definition {
            Some(definition) => match kind {
                NamedTupleKind::Collections => {
                    let spec = self.infer_collections_namedtuple_fields(
                        rename_type,
                        fields_arg,
                        &default_types,
                        defaults_kw,
                    );
                    DynamicNamedTupleAnchor::CollectionsDefinition { definition, spec }
                }
                NamedTupleKind::Typing => {
                    // The `fields` argument to `typing.NamedTuple` cannot be inferred
                    // eagerly if it's not a dangling call, as it may contain forward references
                    // or recursive references.
                    self.deferred.insert(definition);
                    DynamicNamedTupleAnchor::TypingDefinition(definition)
                }
            },
            None => {
                let scope = self.scope();
                let spec = match kind {
                    NamedTupleKind::Collections => self.infer_collections_namedtuple_fields(
                        rename_type,
                        fields_arg,
                        &default_types,
                        defaults_kw,
                    ),
                    NamedTupleKind::Typing => self.infer_typing_namedtuple_fields(fields_arg),
                };
                DynamicNamedTupleAnchor::ScopeOffset {
                    scope,
                    offset: self.dynamic_class_scope_offset(call_expr),
                    spec,
                }
            }
        };

        let namedtuple = DynamicNamedTupleLiteral::new(db, name, anchor);

        Type::ClassLiteral(ClassLiteral::DynamicNamedTuple(namedtuple))
    }

    fn infer_collections_namedtuple_fields(
        &mut self,
        rename_type: Option<Type<'db>>,
        fields_arg: &ast::Expr,
        default_types: &[Type<'db>],
        defaults_kw: Option<&ast::Keyword>,
    ) -> NamedTupleSpec<'db> {
        let env = self.program_environment();
        let db = self.db();

        // `collections.namedtuple`: `field_names` is a list or tuple of strings, or a space or
        // comma-separated string.

        // Check for `rename=True`. Use `is_always_true()` to handle truthy values
        // (e.g., `rename=1`), though we'd still want a diagnostic for non-bool types.
        let rename = rename_type.is_some_and(|ty| ty.bool(db, env).is_always_true());

        let fields_type = self.infer_expression(fields_arg, TypeContext::default());

        // Extract field names, first from the inferred type, then from the AST.
        let maybe_field_names: Option<Box<[Name]>> =
            if let Some(string_literal) = fields_type.as_string_literal() {
                // Handle space/comma-separated string.
                Some(
                    string_literal
                        .value(db)
                        .replace(',', " ")
                        .split_whitespace()
                        .map(Name::new)
                        .collect(),
                )
            } else {
                extract_fixed_length_iterable_element_types(db, env, fields_arg, |expr| {
                    self.expression_type(expr)
                })
                .and_then(|field_types| {
                    field_types
                        .iter()
                        .map(|elt| elt.as_string_literal().map(|s| Name::new(s.value(db))))
                        .collect()
                })
            };

        if maybe_field_names.is_none() {
            // Emit diagnostic if the type is outright invalid (not str | Iterable[str]).
            let iterable_str =
                KnownClass::Iterable.to_specialized_instance(db, env, &[Type::any()]);
            let valid_type = UnionType::from_two_elements(
                db,
                env,
                KnownClass::Str.to_instance(db, env),
                iterable_str,
            );
            if !fields_type.is_assignable_to(db, env, valid_type)
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Invalid argument to parameter `field_names` of `namedtuple()`"
                ));
                diagnostic.set_primary_annotation_message(format_args!(
                    "Expected `str` or an iterable of strings, found `{}`",
                    fields_type.display(db, env)
                ));
            }
        }

        let Some(mut field_names) = maybe_field_names else {
            // Couldn't determine fields statically; attribute lookups will return Any.
            return NamedTupleSpec::unknown(db, Type::any());
        };

        // When `rename` is false (or not specified), emit diagnostics for invalid
        // field names. These all raise ValueError at runtime. When `rename=True`,
        // invalid names are automatically replaced with `_0`, `_1`, etc., so no
        // diagnostic is needed.
        if !rename {
            self.check_invalid_namedtuple_field_names(
                &field_names,
                fields_arg,
                NamedTupleKind::Collections,
            );
        } else {
            // Apply rename logic.
            let mut seen_names = FxHashSet::<&str>::default();
            for (i, field_name) in field_names.iter_mut().enumerate() {
                let name_str = field_name.as_str();
                let needs_rename = name_str.starts_with('_')
                    || is_keyword(name_str)
                    || !is_identifier(name_str)
                    || seen_names.contains(name_str);
                if needs_rename {
                    *field_name = Name::from(format_char!("_{i}"));
                }
                seen_names.insert(field_name.as_str());
            }
        }

        let num_fields = field_names.len();
        let defaults_count = default_types.len();

        if defaults_count > num_fields
            && let Some(defaults_kw) = defaults_kw
            && let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, defaults_kw)
        {
            let mut diagnostic =
                builder.into_diagnostic(format_args!("Too many defaults for `namedtuple()`"));
            diagnostic.set_primary_annotation_message(format_args!(
                "Got {defaults_count} default values but only {num_fields} field names"
            ));
            diagnostic.info("This will raise `TypeError` at runtime");
        }

        let defaults_count = defaults_count.min(num_fields);
        let fields = field_names
            .iter()
            .enumerate()
            .map(|(i, field_name)| {
                let default = if defaults_count > 0 && i >= num_fields - defaults_count {
                    // Index into default_types: first default corresponds to first
                    // field that has a default.
                    let default_idx = i - (num_fields - defaults_count);
                    Some(default_types[default_idx])
                } else {
                    None
                };
                FunctionalNamedTupleField::Named(NamedTupleField {
                    name: field_name.clone(),
                    ty: Type::any(),
                    default,
                    definition: None,
                })
            })
            .collect();

        NamedTupleSpec::from_fields(db, fields)
    }

    pub(super) fn infer_typing_namedtuple_fields(
        &mut self,
        fields_arg: &ast::Expr,
    ) -> NamedTupleSpec<'db> {
        let spec = self.infer_typing_namedtuple_spec(fields_arg);
        let names = spec.named_fields(self.db()).map(|field| &field.name);
        self.check_invalid_namedtuple_field_names(names, fields_arg, NamedTupleKind::Typing);
        spec
    }

    /// Infer the fields of a `typing.NamedTuple` call. For a list or tuple literal, read each
    /// declaration directly. For other expressions, recover the fields from the argument type.
    fn infer_typing_namedtuple_spec(&mut self, fields_arg: &ast::Expr) -> NamedTupleSpec<'db> {
        let db = self.db();

        let Some(elements) = sequence_elts(fields_arg) else {
            let ty = self.infer_expression(fields_arg, TypeContext::default());
            return NamedTupleSpec::from_fields_type(db, self.scope(), ty, |reason| {
                FieldProblem {
                    expression: fields_arg,
                    reason,
                    compatible: self.can_supply_namedtuple_fields(ty),
                }
                .report(self);
            });
        };

        let mut fields = Vec::with_capacity(elements.len());
        let mut problem: Option<FieldProblem<'_>> = None;
        for element in elements {
            // An unknown field name should not hide a later malformed pair.
            if let Some(current) = self.infer_typing_namedtuple_field(element, &mut fields)
                && problem
                    .as_ref()
                    .is_none_or(|previous| previous.compatible && !current.compatible)
            {
                problem = Some(current);
            }
        }
        if let Some(problem) = problem {
            problem.report(self);
        }

        let spec = NamedTupleSpec::from_fields(db, fields.into_boxed_slice());
        self.store_expression_type(
            fields_arg,
            Type::KnownInstance(KnownInstanceType::NamedTupleSpec(spec)),
        );
        spec
    }

    /// Model a list literal as a tuple to retain its length after unpacking.
    /// If `field` is a pair, `[*field, int]` has three items.
    fn field_value_type(&self, value: &ast::Expr) -> Type<'db> {
        match value {
            ast::Expr::List(list) => self.tuple_type_from_literal_elements(&list.elts),
            _ => self.expression_type(value),
        }
    }

    /// Append the fields supplied by a starred expression. Walk nested literals so we can
    /// recover the name and annotation from individual expressions in a pair like `["x", int]`.
    fn unpack_namedtuple_fields(
        &self,
        value: &ast::Expr,
        fields: &mut Vec<FunctionalNamedTupleField<'db>>,
        problem: &mut Option<Cow<'static, str>>,
    ) {
        let db = self.db();
        if let Some(elements) = sequence_elts(value) {
            for element in elements {
                if let ast::Expr::Starred(starred) = element {
                    self.unpack_namedtuple_fields(&starred.value, fields, problem);
                } else {
                    fields.push(NamedTupleSpec::field_from_type(
                        db,
                        self.scope(),
                        self.field_value_type(element),
                        problem,
                    ));
                }
            }
        } else {
            let pairs = self
                .expression_type(value)
                .iterate(db, self.program_environment());
            NamedTupleSpec::extend_fields(db, self.scope(), &pairs, fields, problem);
        }
    }

    /// Append the fields supplied by one element of a literal `fields` argument, returning
    /// any problem with that element.
    fn infer_typing_namedtuple_field<'a>(
        &mut self,
        element: &'a ast::Expr,
        fields: &mut Vec<FunctionalNamedTupleField<'db>>,
    ) -> Option<FieldProblem<'a>> {
        let db = self.db();
        let env = self.program_environment();
        let Some([name_expr, declaration_expr]) = explicit_field_pair(element) else {
            self.infer_expression(element, TypeContext::default());
            let mut problem = None;
            return if let ast::Expr::Starred(starred) = element {
                self.unpack_namedtuple_fields(&starred.value, fields, &mut problem);
                Some(FieldProblem {
                    expression: element,
                    reason: problem?,
                    compatible: self
                        .can_supply_namedtuple_fields(self.field_value_type(&starred.value)),
                })
            } else {
                let pair = self.field_value_type(element);
                fields.push(NamedTupleSpec::field_from_type(
                    db,
                    self.scope(),
                    pair,
                    &mut problem,
                ));
                Some(FieldProblem {
                    expression: element,
                    reason: problem?,
                    compatible: self.can_supply_field_pair(pair),
                })
            };
        };

        let name_type = self.infer_expression(name_expr, TypeContext::default());
        let declared_type = self.infer_type_expression(declaration_expr);
        let element_type = if element.is_tuple_expr() {
            Type::heterogeneous_tuple(db, env, [name_type, declared_type])
        } else {
            KnownClass::List.to_specialized_instance(
                db,
                env,
                &[UnionType::from_two_elements(
                    db,
                    env,
                    name_type,
                    declared_type,
                )],
            )
        };
        self.store_expression_type(element, element_type);

        let name = name_type.resolve_type_alias(db).as_string_literal();
        fields.push(FunctionalNamedTupleField::required(
            name.map(|s| s.value(db)),
            declared_type,
        ));
        name.is_none().then(|| FieldProblem {
            expression: name_expr,
            reason: Cow::Owned(format!(
                "Expected a string literal for the field name, found `{}`",
                name_type.display(db, env)
            )),
            compatible: name_type.is_assignable_to(db, env, KnownClass::Str.to_instance(db, env)),
        })
    }

    /// Check whether the type of `fields` in `NamedTuple("Url", fields)` is compatible with
    /// an iterable of field pairs.
    fn can_supply_namedtuple_fields(&self, ty: Type<'db>) -> bool {
        self.all_iterable_shapes_match(ty, |fields| {
            fields
                .iter_element_types(self.db())
                .all(|pair| self.can_supply_field_pair(pair))
        })
    }

    /// Check whether the type of `field` in `NamedTuple("Url", [field])` is compatible with
    /// a `(name, annotation)` pair.
    fn can_supply_field_pair(&self, ty: Type<'db>) -> bool {
        let db = self.db();
        let env = self.program_environment();
        let str_ty = KnownClass::Str.to_instance(db, env);
        // A string could be a forward reference, and `None` stands for `NoneType`.
        let annotation_ty =
            UnionType::from_elements(db, env, [object_type_form(db), str_ty, Type::none(db, env)]);

        self.all_iterable_shapes_match(ty, |items| {
            if let Some(fixed) = items.as_fixed_length() {
                let [name, annotation] = fixed.all_elements() else {
                    return false;
                };
                return name.is_assignable_to(db, env, str_ty)
                    && annotation.is_assignable_to(db, env, annotation_ty);
            }

            // A `list[str | type]` can contain `["host", str]`. Check whether the iterable
            // can have exactly two items and whether each item's type at that length overlaps
            // the expected type for its position.
            items
                .resize(db, env, TupleLength::Fixed(2))
                .is_ok_and(|pair| {
                    pair.iter_element_types(db)
                        .zip([str_ty, annotation_ty])
                        .all(|(actual, expected)| !actual.is_disjoint_from(db, env, expected))
                })
        })
    }

    /// Check that every alternative of `ty` is iterable and its yielded items satisfy `check`.
    /// Calling `try_iterate` on `tuple[str, type] | tuple[str]` produces a single variable-length
    /// tuple spec that admits two items, hiding the one-item alternative.
    fn all_iterable_shapes_match(
        &self,
        ty: Type<'db>,
        check: impl Fn(&TupleSpec<'db>) -> bool,
    ) -> bool {
        let db = self.db();
        let env = self.program_environment();
        let matches = |ty: Type<'db>| ty.try_iterate(db, env).is_ok_and(|items| check(&items));
        match ty.expand_top_level_aliases(db, env) {
            Type::Union(union) => union.elements(db).iter().copied().all(matches),
            ty => matches(ty),
        }
    }

    /// Report diagnostics for invalid field names in a namedtuple definition.
    fn check_invalid_namedtuple_field_names<'a>(
        &self,
        field_names: impl IntoIterator<Item = &'a Name>,
        fields_arg: &ast::Expr,
        kind: NamedTupleKind,
    ) {
        let mut seen_names = FxHashSet::default();
        for field_name in field_names {
            if !seen_names.insert(field_name)
                && let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Duplicate field name `{field_name}` in `{kind}()`"
                ));
                diagnostic.set_primary_annotation_message(format_args!(
                    "Field `{field_name}` already defined; will raise `ValueError` at runtime"
                ));
            }

            if field_name.starts_with('_')
                && let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Field name `{field_name}` in `{kind}()` cannot start with an underscore"
                ));
                diagnostic.set_primary_annotation_message("Will raise `ValueError` at runtime");
            } else if is_keyword(field_name)
                && let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Field name `{field_name}` in `{kind}()` cannot be a Python keyword"
                ));
                diagnostic.set_primary_annotation_message("Will raise `ValueError` at runtime");
            } else if !is_identifier(field_name)
                && let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Field name `{field_name}` in `{kind}()` is not a valid identifier"
                ));
                diagnostic.set_primary_annotation_message("Will raise `ValueError` at runtime");
            }
        }
    }
}

/// Return the name and annotation expressions from a two-item list or tuple literal with no
/// starred elements.
fn explicit_field_pair(element: &ast::Expr) -> Option<&[ast::Expr; 2]> {
    sequence_elts(element)?
        .as_array::<2>()
        .filter(|parts| parts.iter().all(|part| !part.is_starred_expr()))
}

/// Why ty could not infer the fields of a functional `NamedTuple` definition.
#[derive(Debug)]
struct FieldProblem<'a> {
    expression: &'a ast::Expr,
    reason: Cow<'static, str>,
    /// Whether the inferred type is compatible with the expected field definitions. If so,
    /// report `non-literal-named-tuple-fields`. Otherwise, report `invalid-named-tuple`.
    compatible: bool,
}

impl FieldProblem<'_> {
    fn report(self, inference: &TypeInferenceBuilder<'_, '_>) {
        let (lint, message) = if self.compatible {
            (
                &NON_LITERAL_NAMED_TUPLE_FIELDS,
                "Cannot determine the fields of `NamedTuple()` statically",
            )
        } else {
            (
                &INVALID_NAMED_TUPLE,
                "Invalid argument to parameter `fields` of `NamedTuple()`",
            )
        };
        if let Some(builder) = inference.context.report_lint(lint, self.expression) {
            builder
                .into_diagnostic(message)
                .set_primary_annotation_message(self.reason);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NamedTupleKind {
    Collections,
    Typing,
}

impl NamedTupleKind {
    const fn is_collections(self) -> bool {
        matches!(self, Self::Collections)
    }

    const fn is_typing(self) -> bool {
        matches!(self, Self::Typing)
    }

    pub(super) fn from_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        match ty {
            Type::SpecialForm(SpecialFormType::NamedTuple) => Some(NamedTupleKind::Typing),
            Type::FunctionLiteral(function) => function
                .is_known(db, KnownFunction::NamedTuple)
                .then_some(NamedTupleKind::Collections),
            _ => None,
        }
    }
}

impl std::fmt::Display for NamedTupleKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            NamedTupleKind::Collections => "namedtuple",
            NamedTupleKind::Typing => "NamedTuple",
        })
    }
}
