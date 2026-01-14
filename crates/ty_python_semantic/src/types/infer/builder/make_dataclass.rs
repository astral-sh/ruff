use itertools::Itertools;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, NodeIndex, PythonVersion};
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_python_stdlib::keyword::is_keyword;
use rustc_hash::FxHashSet;

use super::{InferenceRegion, TypeInferenceBuilder, dynamic_class::DynamicClassKind};
use crate::Program;
use crate::types::call::{CallArguments, CallError};
use crate::types::class::{
    ClassLiteral, DataclassFieldSpec, DataclassSpec, DynamicDataclassAnchor,
    DynamicDataclassLiteral, DynamicMetaclassConflict,
};
use crate::types::diagnostic::{
    CYCLIC_CLASS_DEFINITION, DUPLICATE_BASE, INCONSISTENT_MRO, INVALID_ARGUMENT_TYPE,
    INVALID_DATACLASS, IncompatibleBases, MISSING_ARGUMENT, PARAMETER_ALREADY_ASSIGNED,
    TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT, report_conflicting_metaclass_from_bases,
    report_instance_layout_conflict,
};
use crate::types::function::KnownFunction;
use crate::types::mro::DynamicMroErrorKind;
use crate::types::{
    ClassBase, DATACLASS_FLAGS, DataclassFlags, DataclassParams, KnownClass, KnownInstanceType,
    SubclassOfType, Type, TypeContext, UnionType, add_inferred_python_version_hint_to_diagnostic,
};
use ty_python_core::definition::Definition;

struct MakeDataclassDecoratorConfig<'db, 'ast> {
    raw_dataclass_params: DataclassParams<'db>,
    decorator_arg: Option<(&'ast ast::Expr, Type<'db>)>,
    decorator_keyword_types: Vec<(&'ast str, Type<'db>)>,
}

struct MakeDataclassDecoratorResolution<'db> {
    return_ty: Type<'db>,
    effective_dataclass_params: DataclassParams<'db>,
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Report diagnostics for invalid field names in a `make_dataclass()` definition.
    fn check_invalid_make_dataclass_field_names(
        &self,
        field_names: &[Name],
        fields_arg: &ast::Expr,
    ) {
        for (i, field_name) in field_names.iter().enumerate() {
            if field_names[..i].iter().any(|prior| prior == field_name)
                && let Some(builder) = self.context.report_lint(&INVALID_DATACLASS, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Duplicate field name `{field_name}` in `make_dataclass()`"
                ));
                diagnostic.set_primary_message(format_args!(
                    "Field `{field_name}` already defined; will raise `TypeError` at runtime"
                ));
            }

            if is_keyword(field_name)
                && let Some(builder) = self.context.report_lint(&INVALID_DATACLASS, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Field name `{field_name}` in `make_dataclass()` cannot be a Python keyword"
                ));
                diagnostic.set_primary_message("Will raise `TypeError` at runtime");
            } else if !is_identifier(field_name)
                && let Some(builder) = self.context.report_lint(&INVALID_DATACLASS, fields_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Field name `{field_name}` in `make_dataclass()` is not a valid identifier"
                ));
                diagnostic.set_primary_message("Will raise `TypeError` at runtime");
            }
        }
    }

    /// Infer a `dataclasses.make_dataclass(cls_name, fields, ...)` call.
    ///
    /// This method *does not* call `infer_expression` on the object being called;
    /// it is assumed that the type for this AST node has already been inferred before this method is called.
    pub(super) fn infer_make_dataclass_call_expression(
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

        let cls_name_kw = call_expr.arguments.find_keyword("cls_name");
        let fields_kw = call_expr.arguments.find_keyword("fields");

        let (name_arg, fields_arg, rest, name_from_keyword, fields_from_keyword): (
            Option<&ast::Expr>,
            Option<&ast::Expr>,
            &[ast::Expr],
            bool,
            bool,
        ) = match &**args {
            [name, fields, rest @ ..] => (Some(name), Some(fields), rest, false, false),
            [name, rest @ ..] => (
                Some(name),
                fields_kw.map(|kw| &kw.value),
                rest,
                false,
                fields_kw.is_some(),
            ),
            [] => (
                cls_name_kw.map(|kw| &kw.value),
                fields_kw.map(|kw| &kw.value),
                &[],
                cls_name_kw.is_some(),
                fields_kw.is_some(),
            ),
        };

        let (Some(name_arg), Some(fields_arg)) = (name_arg, fields_arg) else {
            for arg in args {
                self.infer_expression(arg, TypeContext::default());
            }
            for kw in keywords {
                self.infer_expression(&kw.value, TypeContext::default());
            }

            if !has_starred && !has_double_starred {
                let missing = match (name_arg.is_none(), fields_arg.is_none()) {
                    (true, true) => "`cls_name` and `fields`",
                    (true, false) => "`cls_name`",
                    (false, true) => "`fields`",
                    (false, false) => unreachable!(),
                };
                let plural = name_arg.is_none() && fields_arg.is_none();
                if let Some(builder) = self.context.report_lint(&MISSING_ARGUMENT, call_expr) {
                    builder.into_diagnostic(format_args!(
                        "No argument{} provided for required parameter{} {missing} of function `make_dataclass`",
                        if plural { "s" } else { "" },
                        if plural { "s" } else { "" }
                    ));
                }
            }

            return SubclassOfType::subclass_of_unknown();
        };

        let name_type = self.infer_expression(name_arg, TypeContext::default());

        for arg in rest {
            self.infer_expression(arg, TypeContext::default());
        }

        if has_starred || has_double_starred {
            for kw in keywords {
                self.infer_expression(&kw.value, TypeContext::default());
            }
            return SubclassOfType::subclass_of_unknown();
        }

        if !rest.is_empty() {
            if let Some(builder) = self
                .context
                .report_lint(&TOO_MANY_POSITIONAL_ARGUMENTS, &rest[0])
            {
                builder.into_diagnostic(format_args!(
                    "Too many positional arguments to function `make_dataclass`: expected 2, got {}",
                    args.len()
                ));
            }
        }

        let mut bases_arg: Option<(&ast::Expr, Type<'db>)> = None;
        let mut namespace_arg: Option<(&ast::Expr, Type<'db>)> = None;
        let mut decorator_keyword_inputs = Vec::new();
        let mut seen_parameters: FxHashSet<&str> = FxHashSet::default();

        if !args.is_empty() {
            seen_parameters.insert("cls_name");
        }
        if args.len() >= 2 {
            seen_parameters.insert("fields");
        }

        for kw in keywords {
            let Some(arg) = &kw.arg else {
                continue;
            };
            let param = arg.id.as_str();

            if matches!(param, "cls_name" | "fields") {
                let already_assigned = !seen_parameters.insert(param);
                if already_assigned {
                    let _ = self.infer_expression(&kw.value, TypeContext::default());
                    if let Some(builder) = self.context.report_lint(&PARAMETER_ALREADY_ASSIGNED, kw)
                    {
                        builder.into_diagnostic(format_args!(
                            "Multiple values provided for parameter `{param}` of function `make_dataclass`"
                        ));
                    }
                    continue;
                }

                if (param == "cls_name" && name_from_keyword)
                    || (param == "fields" && fields_from_keyword)
                {
                    continue;
                }
            }

            let kw_type = self.infer_expression(&kw.value, TypeContext::default());

            if !matches!(param, "cls_name" | "fields") && !seen_parameters.insert(param) {
                if let Some(builder) = self.context.report_lint(&PARAMETER_ALREADY_ASSIGNED, kw) {
                    builder.into_diagnostic(format_args!(
                        "Multiple values provided for parameter `{param}` of function `make_dataclass`"
                    ));
                }
                continue;
            }

            if let Some(min_version) = Self::make_dataclass_keyword_minimum_version(param)
                && !self.make_dataclass_keyword_is_supported(min_version)
            {
                self.report_make_dataclass_unsupported_keyword(kw, param, min_version);
                continue;
            }

            match param {
                "bases" => {
                    bases_arg = Some((&kw.value, kw_type));
                }
                "namespace" => {
                    namespace_arg = Some((&kw.value, kw_type));

                    let dict_type =
                        KnownClass::Dict.to_specialized_instance(db, &[Type::any(), Type::any()]);
                    let valid_type = UnionType::from_elements(db, [dict_type, Type::none(db)]);
                    if !kw_type.is_assignable_to(db, valid_type) {
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, &kw.value)
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Invalid argument to parameter `namespace` of `make_dataclass()`"
                            ));
                            diagnostic.set_primary_message(format_args!(
                                "Expected `dict | None`, found `{}`",
                                kw_type.display(db)
                            ));
                        }
                    }
                }
                "module" => {
                    let valid_type = UnionType::from_elements(
                        db,
                        [KnownClass::Str.to_instance(db), Type::none(db)],
                    );
                    if !kw_type.is_assignable_to(db, valid_type) {
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, &kw.value)
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Invalid argument to parameter `module` of `make_dataclass()`"
                            ));
                            diagnostic.set_primary_message(format_args!(
                                "Expected `str | None`, found `{}`",
                                kw_type.display(db)
                            ));
                        }
                    }
                }
                param if Self::is_make_dataclass_decorator_keyword(param) => {
                    decorator_keyword_inputs.push((param, &kw.value, kw_type));
                }
                unknown_kwarg => {
                    if let Some(builder) = self.context.report_lint(&UNKNOWN_ARGUMENT, kw) {
                        builder.into_diagnostic(format_args!(
                            "Argument `{unknown_kwarg}` does not match any known parameter of function `make_dataclass`",
                        ));
                    }
                }
            }
        }

        let decorator_config = self.make_dataclass_decorator_config(decorator_keyword_inputs, true);
        let raw_dataclass_params = decorator_config.raw_dataclass_params;
        let (members, has_dynamic_namespace) = namespace_arg
            .map(|(namespace_arg, namespace_type)| {
                self.extract_dynamic_namespace_members(namespace_arg, namespace_type, true)
            })
            .unwrap_or_default();

        let name = if let Some(literal) = name_type.as_string_literal() {
            Name::new(literal.value(db))
        } else {
            if !name_type.is_assignable_to(db, KnownClass::Str.to_instance(db))
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Invalid argument to parameter `cls_name` of `make_dataclass()`"
                ));
                diagnostic.set_primary_message(format_args!(
                    "Expected `str`, found `{}`",
                    name_type.display(db)
                ));
            }
            Name::new_static("<unknown>")
        };

        let scope = self.scope();
        let scope_offset = definition.is_none().then(|| {
            let call_node_index = call_expr.node_index.load();
            let scope_anchor = scope.node(db).node_index().unwrap_or(NodeIndex::from(0));
            let anchor_u32 = scope_anchor
                .as_u32()
                .expect("scope anchor should not be NodeIndex::NONE");
            let call_u32 = call_node_index
                .as_u32()
                .expect("call node should not be NodeIndex::NONE");
            call_u32 - anchor_u32
        });

        if let Some(definition) = definition {
            self.deferred.insert(definition);
        }

        let effective_dataclass_params = self.make_dataclass_effective_params(
            &name,
            raw_dataclass_params,
            match definition {
                Some(definition) => DynamicDataclassAnchor::Definition(definition),
                None => DynamicDataclassAnchor::ScopeOffset {
                    scope,
                    offset: scope_offset.expect("dangling make_dataclass should have offset"),
                    spec: DataclassSpec::unknown(db),
                },
            },
            members.clone(),
            has_dynamic_namespace,
            &decorator_config,
        );

        let (anchor, disjoint_bases): (DynamicDataclassAnchor<'db>, IncompatibleBases<'db>) =
            match definition {
                Some(def) => (
                    DynamicDataclassAnchor::Definition(def),
                    IncompatibleBases::default(),
                ),
                None => {
                    let (bases, disjoint_bases) =
                        self.infer_dangling_make_dataclass_bases(bases_arg, &name);
                    let spec = self.infer_dangling_make_dataclass_spec(
                        fields_arg,
                        bases,
                        effective_dataclass_params,
                    );

                    (
                        DynamicDataclassAnchor::ScopeOffset {
                            scope,
                            offset: scope_offset
                                .expect("dangling make_dataclass should have offset"),
                            spec,
                        },
                        disjoint_bases,
                    )
                }
            };

        let dataclass = DynamicDataclassLiteral::new(
            db,
            name,
            effective_dataclass_params,
            anchor,
            members,
            has_dynamic_namespace,
        );

        if definition.is_none() {
            self.check_dynamic_dataclass_mro(dataclass, call_expr, disjoint_bases, bases_arg);
        }

        if let Some((decorator_expr, decorator_ty)) = decorator_config.decorator_arg {
            self.resolve_make_dataclass_decorator(
                decorator_expr,
                decorator_ty,
                Type::ClassLiteral(dataclass.into()),
                &decorator_config.decorator_keyword_types,
                effective_dataclass_params,
                true,
            )
            .return_ty
        } else {
            Type::ClassLiteral(ClassLiteral::DynamicDataclass(dataclass))
        }
    }

    /// Check MRO and instance layout conflicts for a dynamic dataclass.
    /// Used for eager checking of dangling calls.
    fn check_dynamic_dataclass_mro(
        &self,
        dataclass: DynamicDataclassLiteral<'db>,
        call_expr: &ast::ExprCall,
        mut disjoint_bases: IncompatibleBases<'db>,
        bases_arg: Option<(&ast::Expr, Type<'db>)>,
    ) {
        let db = self.db();
        let bases_node = bases_arg.map(|(node, _)| node);

        if report_dynamic_dataclass_mro_errors(&self.context, dataclass, call_expr) {
            disjoint_bases.remove_redundant_entries(db);
            if disjoint_bases.len() > 1 {
                report_instance_layout_conflict(
                    &self.context,
                    dataclass.header_range(db),
                    bases_node.and_then(|n| n.as_tuple_expr().map(|t| t.elts.as_slice())),
                    &disjoint_bases,
                );
            }
        }

        if let Err(DynamicMetaclassConflict {
            metaclass1,
            base1,
            metaclass2,
            base2,
        }) = dataclass.try_metaclass(db)
        {
            report_conflicting_metaclass_from_bases(
                &self.context,
                call_expr.into(),
                dataclass.name(db),
                metaclass1,
                base1.display(db),
                metaclass2,
                base2.display(db),
            );
        }
    }

    fn make_dataclass_decorator_config<'a>(
        &mut self,
        keyword_types: impl IntoIterator<Item = (&'a str, &'a ast::Expr, Type<'db>)>,
        report_invalid_types: bool,
    ) -> MakeDataclassDecoratorConfig<'db, 'a> {
        let db = self.db();
        let bool_type = KnownClass::Bool.to_instance(db);
        let mut dataclass_flags = self.make_dataclass_default_flags();
        let mut decorator_arg = None;
        let mut decorator_keyword_types = Vec::new();

        for (param, keyword_expr, keyword_ty) in keyword_types {
            if param == "decorator" {
                decorator_arg = Some((keyword_expr, keyword_ty));
                continue;
            }

            let Some(flag) = Self::make_dataclass_flag(param) else {
                continue;
            };

            decorator_keyword_types.push((param, keyword_ty));

            if report_invalid_types && !keyword_ty.is_assignable_to(db, bool_type) {
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_ARGUMENT_TYPE, keyword_expr)
                {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Invalid argument to parameter `{param}` of `make_dataclass()`"
                    ));
                    diagnostic.set_primary_message(format_args!(
                        "Expected `bool`, found `{}`",
                        keyword_ty.display(db)
                    ));
                }
            }

            if keyword_ty.bool(db).is_always_true() {
                dataclass_flags.insert(flag);
            } else if keyword_ty.bool(db).is_always_false() {
                dataclass_flags.remove(flag);
            }
        }

        MakeDataclassDecoratorConfig {
            raw_dataclass_params: DataclassParams::from_flags(db, dataclass_flags),
            decorator_arg,
            decorator_keyword_types,
        }
    }

    fn resolve_make_dataclass_decorator(
        &mut self,
        decorator_expr: &ast::Expr,
        decorator_ty: Type<'db>,
        class_ty: Type<'db>,
        decorator_keyword_types: &[(&str, Type<'db>)],
        fallback_dataclass_params: DataclassParams<'db>,
        report_errors: bool,
    ) -> MakeDataclassDecoratorResolution<'db> {
        let call_arguments = CallArguments::positional([class_ty])
            .with_keyword_arguments(decorator_keyword_types.iter().copied());
        let return_ty = decorator_ty
            .try_call(self.db(), &call_arguments)
            .map(|bindings| bindings.return_type(self.db()))
            .unwrap_or_else(|CallError(_, bindings)| {
                if decorator_ty
                    .as_function_literal()
                    .is_some_and(|function| function.is_known(self.db(), KnownFunction::Dataclass))
                    && let Some(return_ty) =
                        class_ty.try_with_dataclass_params(self.db(), fallback_dataclass_params)
                {
                    return return_ty;
                }

                if report_errors {
                    bindings.report_diagnostics(&self.context, decorator_expr.into());
                }
                bindings.return_type(self.db())
            });

        let effective_dataclass_params = match return_ty {
            Type::ClassLiteral(ClassLiteral::DynamicDataclass(dataclass)) => {
                dataclass.dataclass_params(self.db())
            }
            _ => fallback_dataclass_params,
        };

        MakeDataclassDecoratorResolution {
            return_ty,
            effective_dataclass_params,
        }
    }

    fn make_dataclass_effective_params<'a>(
        &mut self,
        name: &Name,
        raw_dataclass_params: DataclassParams<'db>,
        provisional_anchor: DynamicDataclassAnchor<'db>,
        members: Box<[(Name, Type<'db>)]>,
        has_dynamic_namespace: bool,
        decorator_config: &MakeDataclassDecoratorConfig<'db, 'a>,
    ) -> DataclassParams<'db> {
        let Some((decorator_expr, decorator_ty)) = decorator_config.decorator_arg else {
            return raw_dataclass_params;
        };

        let provisional_dataclass = DynamicDataclassLiteral::new(
            self.db(),
            name.clone(),
            raw_dataclass_params,
            provisional_anchor,
            members,
            has_dynamic_namespace,
        );

        self.resolve_make_dataclass_decorator(
            decorator_expr,
            decorator_ty,
            Type::ClassLiteral(provisional_dataclass.into()),
            &decorator_config.decorator_keyword_types,
            raw_dataclass_params,
            false,
        )
        .effective_dataclass_params
    }

    fn make_dataclass_flag(keyword: &str) -> Option<DataclassFlags> {
        DATACLASS_FLAGS
            .iter()
            .find_map(|(flag_name, flag)| (*flag_name == keyword).then_some(*flag))
    }

    fn is_make_dataclass_decorator_keyword(keyword: &str) -> bool {
        keyword == "decorator" || Self::make_dataclass_flag(keyword).is_some()
    }

    /// Infer deferred field and base types for a `make_dataclass()` assignment.
    ///
    /// This is called during deferred evaluation to process forward references
    /// and recursive types in field type annotations and base classes.
    pub(super) fn infer_make_dataclass_deferred(&mut self, arguments: &ast::Arguments) {
        let db = self.db();
        let Some(name_arg) = arguments
            .args
            .first()
            .or_else(|| arguments.find_keyword("cls_name").map(|kw| &kw.value))
        else {
            return;
        };
        let Some(fields_arg) = arguments
            .args
            .get(1)
            .or_else(|| arguments.find_keyword("fields").map(|kw| &kw.value))
        else {
            return;
        };

        let name_type = self
            .try_expression_type(name_arg)
            .unwrap_or_else(|| self.infer_expression(name_arg, TypeContext::default()));
        let name = if let Some(literal) = name_type.as_string_literal() {
            Name::new(literal.value(db))
        } else {
            Name::new_static("<dataclass>")
        };

        let InferenceRegion::Deferred(definition) = self.region else {
            return;
        };
        let previous_context = self.typevar_binding_context.replace(definition);

        let decorator_keyword_inputs: Vec<_> = arguments
            .keywords
            .iter()
            .filter_map(|keyword| {
                let argument_name = keyword.arg.as_ref()?;
                let param = argument_name.as_str();
                if !Self::is_make_dataclass_decorator_keyword(param) {
                    return None;
                }

                if Self::make_dataclass_keyword_minimum_version(param).is_some_and(
                    |minimum_version| !self.make_dataclass_keyword_is_supported(minimum_version),
                ) {
                    return None;
                }

                let keyword_ty = self.try_expression_type(&keyword.value).unwrap_or_else(|| {
                    self.infer_expression(&keyword.value, TypeContext::default())
                });
                Some((param, &keyword.value, keyword_ty))
            })
            .collect();
        let decorator_config =
            self.make_dataclass_decorator_config(decorator_keyword_inputs, false);
        let raw_dataclass_params = decorator_config.raw_dataclass_params;
        let effective_dataclass_params = self.make_dataclass_effective_params(
            &name,
            raw_dataclass_params,
            DynamicDataclassAnchor::Definition(definition),
            Box::default(),
            false,
            &decorator_config,
        );

        let bases: Box<[ClassBase<'db>]> = if let Some(bases_kw) = arguments.find_keyword("bases") {
            self.infer_expression(&bases_kw.value, TypeContext::default());
            let bases_type = self.expression_type(&bases_kw.value);
            self.resolve_make_dataclass_bases(&bases_kw.value, bases_type, &name)
                .0
        } else {
            Box::default()
        };

        self.infer_make_dataclass_fields(fields_arg, bases, effective_dataclass_params);
        self.typevar_binding_context = previous_context;
    }

    /// Infer the field and base specification for an inline `make_dataclass(...)` call.
    ///
    /// This mirrors `infer_dangling_typeddict_spec`: the main call path has already validated the
    /// non-type parts of the `fields` argument, so this helper just computes the eagerly available
    /// spec preserved on the `ScopeOffset` anchor.
    fn infer_dangling_make_dataclass_spec(
        &mut self,
        fields_arg: &ast::Expr,
        bases: Box<[ClassBase<'db>]>,
        dataclass_params: DataclassParams<'db>,
    ) -> DataclassSpec<'db> {
        self.infer_make_dataclass_fields(fields_arg, bases, dataclass_params)
    }

    fn infer_dangling_make_dataclass_bases(
        &mut self,
        bases_arg: Option<(&ast::Expr, Type<'db>)>,
        name: &Name,
    ) -> (Box<[ClassBase<'db>]>, IncompatibleBases<'db>) {
        if let Some((bases_node, bases_type)) = bases_arg {
            self.resolve_make_dataclass_bases(bases_node, bases_type, name)
        } else {
            (Box::default(), IncompatibleBases::default())
        }
    }

    fn resolve_make_dataclass_bases(
        &mut self,
        bases_node: &ast::Expr,
        bases_type: Type<'db>,
        name: &Name,
    ) -> (Box<[ClassBase<'db>]>, IncompatibleBases<'db>) {
        let db = self.db();
        let Some(explicit_bases) =
            self.extract_explicit_bases(bases_node, bases_type, DynamicClassKind::MakeDataclass)
        else {
            return (
                Box::from([ClassBase::unknown()]),
                IncompatibleBases::default(),
            );
        };

        let disjoint = self.validate_dynamic_type_bases(
            bases_node,
            &explicit_bases,
            name,
            DynamicClassKind::MakeDataclass,
        );
        let class_bases = explicit_bases
            .iter()
            .filter_map(|ty| ClassBase::try_from_type(db, *ty, None))
            .collect();
        (class_bases, disjoint)
    }

    /// Infer fields from a `make_dataclass` fields argument.
    ///
    /// This method properly handles string annotations as forward references by using
    /// `infer_type_expression` instead of `expression_type().in_type_expression()`.
    ///
    /// Returns a `DataclassSpec` containing the fields. The spec is also stored as the
    /// expression type of the fields argument so it can be retrieved during deferred evaluation.
    fn infer_make_dataclass_fields(
        &mut self,
        fields_arg: &ast::Expr,
        bases: Box<[ClassBase<'db>]>,
        dataclass_params: DataclassParams<'db>,
    ) -> DataclassSpec<'db> {
        let db = self.db();
        let field_specifiers = dataclass_params.field_specifiers(db);
        let respect_field_specifier_metadata = !field_specifiers.is_empty();
        self.with_dataclass_field_specifiers(field_specifiers, |this| {
            let store_unknown_spec = |builder: &mut Self| {
                let spec = DataclassSpec::unknown(db);
                builder.store_expression_type(
                    fields_arg,
                    Type::KnownInstance(KnownInstanceType::DataclassSpec(spec)),
                );
                spec
            };

            let elements: &[ast::Expr] = match fields_arg {
                ast::Expr::List(list) => &list.elts,
                ast::Expr::Tuple(tuple) => &tuple.elts,
                _ => {
                    this.infer_expression(fields_arg, TypeContext::default());
                    return DataclassSpec::unknown(db);
                }
            };

            let mut fields = Vec::with_capacity(elements.len());
            let mut has_dynamic_fields = false;

            for elt in elements {
                if let ast::Expr::StringLiteral(string_lit) = elt {
                    let name = Name::new(string_lit.value.to_str());
                    fields.push(DataclassFieldSpec {
                        name,
                        ty: Type::any(),
                        default_ty: None,
                        init: true,
                        kw_only: None,
                        alias: None,
                    });
                    this.store_expression_type(
                        elt,
                        Type::string_literal(db, string_lit.value.to_str()),
                    );
                    continue;
                }

                let field_elements: &[ast::Expr] = match elt {
                    ast::Expr::Tuple(tuple) => &tuple.elts,
                    ast::Expr::List(list) => &list.elts,
                    _ => {
                        this.infer_expression(elt, TypeContext::default());
                        has_dynamic_fields = true;
                        continue;
                    }
                };

                match field_elements {
                    [name_expr, type_expr] => {
                        let name_ty = this.infer_expression(name_expr, TypeContext::default());
                        let field_ty = this.infer_type_expression(type_expr);

                        if let Some(name_lit) = name_ty.as_string_literal() {
                            let field_name = Name::new(name_lit.value(db));
                            fields.push(DataclassFieldSpec {
                                name: field_name,
                                ty: field_ty,
                                default_ty: None,
                                init: true,
                                kw_only: None,
                                alias: None,
                            });
                        } else if !name_ty.is_assignable_to(db, KnownClass::Str.to_instance(db)) {
                            if let Some(diagnostic_builder) =
                                this.context.report_lint(&INVALID_DATACLASS, name_expr)
                            {
                                let mut diagnostic = diagnostic_builder.into_diagnostic(
                                    "Invalid `make_dataclass` field name definition",
                                );
                                diagnostic.set_primary_message(format_args!(
                                    "Expected `str`, found `{}`",
                                    name_ty.display(db)
                                ));
                            }
                            return store_unknown_spec(this);
                        } else {
                            has_dynamic_fields = true;
                        }
                        this.store_expression_type(
                            elt,
                            Type::heterogeneous_tuple(db, [name_ty, field_ty]),
                        );
                    }
                    [name_expr, type_expr, default_expr] => {
                        let name_ty = this.infer_expression(name_expr, TypeContext::default());
                        let field_ty = this.infer_type_expression(type_expr);
                        let default_ty_value =
                            this.infer_expression(default_expr, TypeContext::default());

                        if let Some(name_lit) = name_ty.as_string_literal() {
                            let field_name = Name::new(name_lit.value(db));
                            let (default_ty, init, kw_only, alias) =
                                if let Type::KnownInstance(KnownInstanceType::Field(field)) =
                                    default_ty_value
                                {
                                    let default_ty = field.default_type(db);
                                    if respect_field_specifier_metadata {
                                        (
                                            default_ty,
                                            field.init(db),
                                            field.kw_only(db),
                                            field.alias(db).map(Name::new),
                                        )
                                    } else {
                                        (default_ty, true, None, None)
                                    }
                                } else {
                                    (Some(default_ty_value), true, None, None)
                                };
                            fields.push(DataclassFieldSpec {
                                name: field_name,
                                ty: field_ty,
                                default_ty,
                                init,
                                kw_only,
                                alias,
                            });
                        } else if !name_ty.is_assignable_to(db, KnownClass::Str.to_instance(db)) {
                            if let Some(diagnostic_builder) =
                                this.context.report_lint(&INVALID_DATACLASS, name_expr)
                            {
                                let mut diagnostic = diagnostic_builder.into_diagnostic(
                                    "Invalid `make_dataclass` field name definition",
                                );
                                diagnostic.set_primary_message(format_args!(
                                    "Expected `str`, found `{}`",
                                    name_ty.display(db)
                                ));
                            }
                            return store_unknown_spec(this);
                        } else {
                            has_dynamic_fields = true;
                        }
                        this.store_expression_type(
                            elt,
                            Type::heterogeneous_tuple(db, [name_ty, field_ty, default_ty_value]),
                        );
                    }
                    _ => {
                        for expr in field_elements {
                            this.infer_expression(expr, TypeContext::default());
                        }
                        if let Some(diagnostic_builder) =
                            this.context.report_lint(&INVALID_DATACLASS, elt)
                        {
                            let mut diagnostic = diagnostic_builder.into_diagnostic(format_args!(
                                "Invalid field definition in `make_dataclass()`"
                            ));
                            diagnostic.set_primary_message(
                                "Each field must be a string, or a length-2 or length-3 tuple/list",
                            );
                        }
                        return store_unknown_spec(this);
                    }
                }
            }

            let field_names: Vec<Name> = fields.iter().map(|field| field.name.clone()).collect();
            this.check_invalid_make_dataclass_field_names(&field_names, fields_arg);

            if has_dynamic_fields {
                return store_unknown_spec(this);
            }

            let spec = DataclassSpec::known(db, fields.into_boxed_slice(), bases);
            this.store_expression_type(
                fields_arg,
                Type::KnownInstance(KnownInstanceType::DataclassSpec(spec)),
            );
            spec
        })
    }

    fn make_dataclass_default_flags(&self) -> DataclassFlags {
        let mut flags = DataclassFlags::INIT | DataclassFlags::REPR | DataclassFlags::EQ;
        if self.in_stub()
            || Program::get(self.db()).python_version(self.db()) >= PythonVersion::PY310
        {
            flags.insert(DataclassFlags::MATCH_ARGS);
        }
        flags
    }

    fn make_dataclass_keyword_minimum_version(keyword: &str) -> Option<PythonVersion> {
        match keyword {
            "match_args" | "kw_only" | "slots" => Some(PythonVersion::PY310),
            "weakref_slot" => Some(PythonVersion::PY311),
            "module" => Some(PythonVersion::PY312),
            "decorator" => Some(PythonVersion::PY314),
            _ => None,
        }
    }

    fn make_dataclass_keyword_is_supported(&self, minimum_version: PythonVersion) -> bool {
        self.in_stub() || Program::get(self.db()).python_version(self.db()) >= minimum_version
    }

    fn report_make_dataclass_unsupported_keyword(
        &self,
        keyword: &ast::Keyword,
        parameter: &str,
        minimum_version: PythonVersion,
    ) {
        let Some(builder) = self.context.report_lint(&UNKNOWN_ARGUMENT, keyword) else {
            return;
        };

        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Argument `{parameter}` does not match any known parameter of function `make_dataclass`",
        ));
        diagnostic.info(format_args!(
            "The `{parameter}` parameter is only available on Python {minimum_version}+",
        ));
        add_inferred_python_version_hint_to_diagnostic(
            self.db(),
            &mut diagnostic,
            "resolving types",
        );
    }
}

/// Report MRO errors for a dynamic dataclass created via `make_dataclass()`.
///
/// Returns `true` if the MRO is valid (no errors), `false` if errors were reported.
/// This is used both for eager checking (dangling calls) and deferred checking (assigned calls).
pub(in super::super) fn report_dynamic_dataclass_mro_errors<'db>(
    context: &crate::types::context::InferContext<'db, '_>,
    dataclass: DynamicDataclassLiteral<'db>,
    call_expr: &ast::ExprCall,
) -> bool {
    let db = context.db();
    let Err(error) = dataclass.try_mro(db) else {
        return true;
    };

    match error.reason() {
        DynamicMroErrorKind::InvalidBases(_) => {
            // DynamicDataclassLiteral bases are already resolved as ClassBase,
            // so InvalidBases should not occur.
        }
        DynamicMroErrorKind::InheritanceCycle => {
            if let Some(builder) = context.report_lint(&CYCLIC_CLASS_DEFINITION, call_expr) {
                builder.into_diagnostic(format_args!(
                    "Cyclic definition of `{}`",
                    dataclass.name(db)
                ));
            }
        }
        DynamicMroErrorKind::DuplicateBases(duplicates) => {
            if let Some(builder) = context.report_lint(&DUPLICATE_BASE, call_expr) {
                builder.into_diagnostic(format_args!(
                    "Duplicate base class{maybe_s} {dupes} in class `{class}`",
                    maybe_s = if duplicates.len() == 1 { "" } else { "es" },
                    dupes = duplicates
                        .iter()
                        .map(|base: &ClassBase<'_>| base.display(db))
                        .join(", "),
                    class = dataclass.name(db),
                ));
            }
        }
        DynamicMroErrorKind::UnresolvableMro => {
            if let Some(builder) = context.report_lint(&INCONSISTENT_MRO, call_expr) {
                builder.into_diagnostic(format_args!(
                    "Cannot create a consistent method resolution order (MRO) \
                        for class `{}` with bases `[{}]`",
                    dataclass.name(db),
                    dataclass
                        .bases(db)
                        .iter()
                        .map(|base| base.display(db))
                        .join(", ")
                ));
            }
        }
    }

    false
}
