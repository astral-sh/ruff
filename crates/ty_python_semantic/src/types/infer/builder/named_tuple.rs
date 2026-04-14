use crate::{
    Db,
    types::{
        ClassLiteral, IntersectionType, KnownClass, KnownInstanceType, SpecialFormType, Type,
        TypeContext, UnionType,
        class::{
            DynamicNamedTupleAnchor, DynamicNamedTupleLiteral, NamedTupleField, NamedTupleSpec,
        },
        diagnostic::{
            INVALID_ARGUMENT_TYPE, INVALID_NAMED_TUPLE, MISSING_ARGUMENT,
            PARAMETER_ALREADY_ASSIGNED, TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
            report_mismatched_type_name,
        },
        extract_fixed_length_iterable_element_types,
        function::KnownFunction,
        infer::TypeInferenceBuilder,
    },
};
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
                [
                    Type::homogeneous_tuple(db, Type::unknown()).to_meta_type(db),
                    KnownClass::NamedTupleLike.to_subclass_of(db),
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
                if let Some(arg) = kw.arg.as_ref() {
                    if name_from_keyword && arg.id.as_str() == "typename" {
                        continue;
                    }
                    if fields_from_keyword && arg.id.as_str() == "field_names" {
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
                        extract_fixed_length_iterable_element_types(db, &kw.value, |expr| {
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
                        KnownClass::Iterable.to_specialized_instance(db, &[Type::any()]);
                    let valid_type = UnionType::from_two_elements(db, iterable_any, Type::none(db));
                    if !kw_type.is_assignable_to(db, valid_type)
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, &kw.value)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Invalid argument to parameter `defaults` of `namedtuple()`"
                        ));
                        diagnostic.set_primary_message(format_args!(
                            "Expected `Iterable[Any] | None`, found `{}`",
                            kw_type.display(db)
                        ));
                    }
                }
                "rename" if kind.is_collections() => {
                    rename_type = Some(kw_type);

                    // Emit diagnostic for non-bool types.
                    if !kw_type.is_assignable_to(db, KnownClass::Bool.to_instance(db))
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, &kw.value)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Invalid argument to parameter `rename` of `namedtuple()`"
                        ));
                        diagnostic.set_primary_message(format_args!(
                            "Expected `bool`, found `{}`",
                            kw_type.display(db)
                        ));
                    }
                }
                "module" if kind.is_collections() => {
                    // Emit diagnostic for invalid types (not str | None).
                    let valid_type = UnionType::from_two_elements(
                        db,
                        KnownClass::Str.to_instance(db),
                        Type::none(db),
                    );
                    if !kw_type.is_assignable_to(db, valid_type)
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, &kw.value)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Invalid argument to parameter `module` of `namedtuple()`"
                        ));
                        diagnostic.set_primary_message(format_args!(
                            "Expected `str | None`, found `{}`",
                            kw_type.display(db)
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
            .map(|literal| Name::new(literal.value(db)));

        if name.is_none()
            && !name_type.is_assignable_to(db, KnownClass::Str.to_instance(db))
            && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
        {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Invalid argument to parameter `typename` of `{kind}()`"
            ));
            diagnostic.set_primary_message(format_args!(
                "Expected `str`, found `{}`",
                name_type.display(db)
            ));
        } else if let Some(actual_name) = name.as_deref()
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

        let name = name.unwrap_or_else(|| Name::new_static("<unknown>"));

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
                let call_node_index = call_expr.node_index.load();
                let scope = self.scope();
                let scope_anchor = scope
                    .node(db)
                    .node_index()
                    .unwrap_or(ast::NodeIndex::from(0));
                let anchor_u32 = scope_anchor
                    .as_u32()
                    .expect("scope anchor should not be NodeIndex::NONE");
                let call_u32 = call_node_index
                    .as_u32()
                    .expect("call node should not be NodeIndex::NONE");
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
                    offset: call_u32 - anchor_u32,
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
        let db = self.db();

        // `collections.namedtuple`: `field_names` is a list or tuple of strings, or a space or
        // comma-separated string.

        // Check for `rename=True`. Use `is_always_true()` to handle truthy values
        // (e.g., `rename=1`), though we'd still want a diagnostic for non-bool types.
        let rename = rename_type.is_some_and(|ty| ty.bool(db).is_always_true());

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
                extract_fixed_length_iterable_element_types(db, fields_arg, |expr| {
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
            let iterable_str = KnownClass::Iterable.to_specialized_instance(db, &[Type::any()]);
            let valid_type =
                UnionType::from_two_elements(db, KnownClass::Str.to_instance(db), iterable_str);
            if !fields_type.is_assignable_to(db, valid_type)
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Invalid argument to parameter `field_names` of `namedtuple()`"
                ));
                diagnostic.set_primary_message(format_args!(
                    "Expected `str` or an iterable of strings, found `{}`",
                    fields_type.display(db)
                ));
            }
        }

        let Some(mut field_names) = maybe_field_names else {
            // Couldn't determine fields statically; attribute lookups will return Any.
            return NamedTupleSpec::unknown(db);
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
                    *field_name = Name::new(format!("_{i}"));
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
            diagnostic.set_primary_message(format_args!(
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
                NamedTupleField {
                    name: field_name.clone(),
                    ty: Type::any(),
                    default,
                }
            })
            .collect();

        NamedTupleSpec::known(db, fields)
    }

    pub(super) fn infer_typing_namedtuple_fields(
        &mut self,
        fields_arg: &ast::Expr,
    ) -> NamedTupleSpec<'db> {
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        enum SequenceKind {
            List,
            Tuple,
        }

        let db = self.db();

        // Get the elements from the list or tuple literal.
        let (elements, field_arg_kind) = match fields_arg {
            ast::Expr::List(list) => (&list.elts, SequenceKind::List),
            ast::Expr::Tuple(tuple) => (&tuple.elts, SequenceKind::Tuple),
            _ => {
                self.infer_expression(fields_arg, TypeContext::default());
                if let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg) {
                    let mut diagnostic = builder.into_diagnostic(
                        "Invalid argument to parameter `fields` of `NamedTuple()`",
                    );
                    diagnostic.set_primary_message("`fields` must be a literal list or tuple");
                }
                return NamedTupleSpec::unknown(db);
            }
        };

        let mut fields = vec![];

        for (i, element) in elements.iter().enumerate() {
            // Each element should be a tuple or list like ("field_name", type) or ["field_name", type].
            let (field_spec_elts, field_spec_kind) = match element {
                ast::Expr::Tuple(tuple) => (&tuple.elts, SequenceKind::Tuple),
                ast::Expr::List(list) => (&list.elts, SequenceKind::List),
                _ => {
                    self.infer_expression(element, TypeContext::default());
                    for element in &elements[(i + 1)..] {
                        self.infer_expression(element, TypeContext::default());
                    }
                    match field_arg_kind {
                        SequenceKind::List => {
                            self.store_expression_type(
                                fields_arg,
                                KnownClass::List.to_instance(db),
                            );
                        }
                        SequenceKind::Tuple => self.store_expression_type(
                            fields_arg,
                            Type::homogeneous_tuple(db, Type::unknown()),
                        ),
                    }
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg)
                    {
                        let mut diagnostic = builder.into_diagnostic(
                            "Invalid argument to parameter `fields` of `NamedTuple()`",
                        );
                        diagnostic.set_primary_message(
                            "`fields` must be a sequence of literal lists or tuples",
                        );
                    }
                    return NamedTupleSpec::unknown(db);
                }
            };

            let [name_expr, declaration_expr] = &**field_spec_elts else {
                self.infer_expression(element, TypeContext::default());
                for element in &elements[(i + 1)..] {
                    self.infer_expression(element, TypeContext::default());
                }
                match field_arg_kind {
                    SequenceKind::List => {
                        self.store_expression_type(fields_arg, KnownClass::List.to_instance(db));
                    }
                    SequenceKind::Tuple => self.store_expression_type(
                        fields_arg,
                        Type::homogeneous_tuple(db, Type::unknown()),
                    ),
                }
                if let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg) {
                    let mut diagnostic = builder.into_diagnostic(
                        "Invalid argument to parameter `fields` of `NamedTuple()`",
                    );
                    diagnostic.set_primary_message(
                        "Each element in `fields` must be a length-2 tuple or list",
                    );
                }
                return NamedTupleSpec::unknown(db);
            };

            let name_type = self.infer_expression(name_expr, TypeContext::default());
            let declared_type = self.infer_type_expression(declaration_expr);

            let element_type = match field_spec_kind {
                SequenceKind::Tuple => Type::heterogeneous_tuple(db, [name_type, declared_type]),
                SequenceKind::List => KnownClass::List.to_specialized_instance(
                    db,
                    &[UnionType::from_two_elements(db, name_type, declared_type)],
                ),
            };

            self.store_expression_type(element, element_type);

            let Some(name) = name_type.as_string_literal() else {
                for element in &elements[(i + 1)..] {
                    self.infer_expression(element, TypeContext::default());
                }
                match field_arg_kind {
                    SequenceKind::List => {
                        self.store_expression_type(fields_arg, KnownClass::List.to_instance(db));
                    }
                    SequenceKind::Tuple => self.store_expression_type(
                        fields_arg,
                        Type::homogeneous_tuple(db, Type::unknown()),
                    ),
                }
                if let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, name_expr) {
                    let mut diagnostic =
                        builder.into_diagnostic("Invalid `NamedTuple` field name definition");
                    diagnostic.set_primary_message(format_args!(
                        "Expected a string literal for the field name, found `{}`",
                        name_type.display(db)
                    ));
                }
                return NamedTupleSpec::unknown(db);
            };

            let field = NamedTupleField {
                name: Name::new(name.value(db)),
                ty: declared_type,
                default: None,
            };

            fields.push(field);
        }

        let names: Vec<Name> = fields.iter().map(|f| f.name.clone()).collect();

        self.check_invalid_namedtuple_field_names(&names, fields_arg, NamedTupleKind::Typing);

        let spec = NamedTupleSpec::known(db, fields.into_boxed_slice());
        self.store_expression_type(
            fields_arg,
            Type::KnownInstance(KnownInstanceType::NamedTupleSpec(spec)),
        );
        spec
    }

    /// Report diagnostics for invalid field names in a namedtuple definition.
    fn check_invalid_namedtuple_field_names(
        &self,
        field_names: &[Name],
        fields_arg: &ast::Expr,
        kind: NamedTupleKind,
    ) {
        for (i, field_name) in field_names.iter().enumerate() {
            // Check for duplicate field names.
            if field_names[..i].iter().any(|f| f == field_name)
                && let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Duplicate field name `{field_name}` in `{kind}()`"
                ));
                diagnostic.set_primary_message(format_args!(
                    "Field `{field_name}` already defined; will raise `ValueError` at runtime"
                ));
            }

            if field_name.starts_with('_')
                && let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Field name `{field_name}` in `{kind}()` cannot start with an underscore"
                ));
                diagnostic.set_primary_message("Will raise `ValueError` at runtime");
            } else if is_keyword(field_name)
                && let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Field name `{field_name}` in `{kind}()` cannot be a Python keyword"
                ));
                diagnostic.set_primary_message("Will raise `ValueError` at runtime");
            } else if !is_identifier(field_name)
                && let Some(builder) = self.context.report_lint(&INVALID_NAMED_TUPLE, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Field name `{field_name}` in `{kind}()` is not a valid identifier"
                ));
                diagnostic.set_primary_message("Will raise `ValueError` at runtime");
            }
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
