use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, NodeIndex, PythonVersion};
use rustc_hash::FxHashSet;

use crate::{
    Db, Program,
    semantic_index::definition::Definition,
    types::{
        ClassLiteral, KnownClass, Type, TypeContext,
        class::{DynamicEnumAnchor, DynamicEnumLiteral, EnumSpec},
        diagnostic::{
            INVALID_ARGUMENT_TYPE, INVALID_BASE, TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
        },
        infer::TypeInferenceBuilder,
        infer::builder::dynamic_class::report_mro_error_kind,
        subclass_of::SubclassOfType,
    },
};

/// Find enum base class of the given type.
pub(crate) fn enum_functional_call_base<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<KnownClass> {
    let ClassLiteral::Static(cls) = ty.as_class_literal()? else {
        return None;
    };
    cls.known(db).filter(|k| {
        matches!(
            k,
            KnownClass::Enum
                | KnownClass::StrEnum
                | KnownClass::IntEnum
                | KnownClass::Flag
                | KnownClass::IntFlag
        )
    })
}

fn enum_functional_call_keyword_is_valid(name: &str, python_version: PythonVersion) -> bool {
    matches!(
        name,
        "value" | "names" | "start" | "type" | "module" | "qualname"
    ) || (name == "boundary" && python_version >= PythonVersion::PY311)
}

fn has_duplicate_enum_member_names<'db>(members: &[(Name, Type<'db>)]) -> bool {
    let mut seen = FxHashSet::default();
    members.iter().any(|(name, _)| !seen.insert(name.clone()))
}

/// Compute the auto-assigned value for the `i`-th enum member.
///
/// `StrEnum` uses the lowercased member name, `Flag`/`IntFlag` use powers
/// of two (`start << i`), and all others use sequential integers (`start + i`).
fn enum_auto_value<'db>(
    db: &'db dyn Db,
    base_class: KnownClass,
    name: &str,
    start: i64,
    index: usize,
) -> Type<'db> {
    match base_class {
        KnownClass::StrEnum => Type::string_literal(db, &name.to_lowercase()),
        KnownClass::Flag | KnownClass::IntFlag => {
            let shift = i64::try_from(index).ok();
            let headroom = if start >= 0 {
                start.leading_zeros().saturating_sub(1)
            } else {
                start.leading_ones().saturating_sub(1)
            };
            shift
                .and_then(|s| u32::try_from(s).ok())
                .filter(|&s| s <= headroom)
                .and_then(|s| start.checked_shl(s))
                .map(Type::int_literal)
                .unwrap_or_else(|| KnownClass::Int.to_instance(db))
        }
        _ => i64::try_from(index)
            .ok()
            .and_then(|i| start.checked_add(i))
            .map(Type::int_literal)
            .unwrap_or_else(|| KnownClass::Int.to_instance(db)),
    }
}

/// Compute the next auto-assigned value based on the previous member's value.
///
/// Used for dict-form and tuple/list-form functional enums where `auto()` values
/// are derived from the predecessor rather than positional index.
/// - `StrEnum`: lowercased member name
/// - `Flag`/`IntFlag`: next highest power of two
/// - Others: `last_value + 1`
fn next_auto_value<'db>(
    db: &'db dyn Db,
    base_class: KnownClass,
    name: &str,
    last_int_value: Option<i64>,
) -> Type<'db> {
    match base_class {
        KnownClass::StrEnum => Type::string_literal(db, &name.to_lowercase()),
        _ => {
            let Some(last) = last_int_value else {
                return KnownClass::Int.to_instance(db);
            };
            match base_class {
                KnownClass::Flag | KnownClass::IntFlag => {
                    // next power of two after highest bit in the last value
                    if last <= 0 {
                        Type::int_literal(1)
                    } else {
                        let shift = i64::BITS - last.leading_zeros();
                        1_i64
                            .checked_shl(shift)
                            .map(Type::int_literal)
                            .unwrap_or_else(|| KnownClass::Int.to_instance(db))
                    }
                }
                _ => last
                    .checked_add(1)
                    .map(Type::int_literal)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(db)),
            }
        }
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(crate) fn infer_enum_call_expression(
        &mut self,
        call_expr: &ast::ExprCall,
        definition: Option<Definition<'db>>,
        base_class: KnownClass,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let ast::Arguments {
            args,
            keywords,
            range: _,
            node_index: _,
        } = &call_expr.arguments;

        let base_name = base_class.name(db);
        let python_version = Program::get(db).python_version(db);

        for kw in keywords {
            if let Some(name) = &kw.arg
                && !enum_functional_call_keyword_is_valid(name.as_str(), python_version)
                && let Some(builder) = self.context.report_lint(&UNKNOWN_ARGUMENT, kw)
            {
                builder.into_diagnostic(format_args!(
                    "Argument `{name}` does not match any known parameter of function `{base_name}`",
                ));
            }
        }

        let value_kw = call_expr.arguments.find_keyword("value");
        let names_kw = call_expr.arguments.find_keyword("names");
        let start_kw = call_expr.arguments.find_keyword("start");
        let type_kw = call_expr.arguments.find_keyword("type");

        // Bail out on positional/keyword conflicts so normal binding can diagnose them.
        let has_positional_keyword_conflict =
            (!args.is_empty() && value_kw.is_some()) || (args.len() >= 2 && names_kw.is_some());
        if has_positional_keyword_conflict {
            return None;
        }

        let (name_arg, names_arg): (Option<&ast::Expr>, Option<&ast::Expr>) = match &**args {
            [name, names, ..] => (Some(name), Some(names)),
            [name] => (Some(name), names_kw.map(|kw| &kw.value)),
            [] => (value_kw.map(|kw| &kw.value), names_kw.map(|kw| &kw.value)),
        };

        let name_arg = name_arg?;

        for arg in args {
            self.infer_expression(arg, TypeContext::default());
        }
        for kw in keywords {
            self.infer_expression(&kw.value, TypeContext::default());
        }

        // Non-literal names use the ordinary `type[EnumSubclass]` overload result
        // instead of synthesizing a `DynamicEnumLiteral`.
        let Some(name) = self.infer_enum_name_argument(name_arg, base_class) else {
            return SubclassOfType::try_from_type(db, base_class.to_class_literal(db));
        };

        let start = start_kw.map_or(1, |kw| self.infer_enum_start_argument(&kw.value));
        let mixin_type = type_kw.map(|kw| self.infer_enum_mixin_argument(&kw.value));

        // Only 1 extra positional arg is allowed (the `names` parameter).
        // `Enum("Color", "RED", "GREEN")` is invalid at runtime.
        let has_too_many_positional = args.len() > 2;
        if has_too_many_positional
            && let Some(builder) = self
                .context
                .report_lint(&TOO_MANY_POSITIONAL_ARGUMENTS, &args[2])
        {
            builder.into_diagnostic(format_args!(
                "Too many positional arguments to function `{base_name}`: expected 2, got {}",
                args.len(),
            ));
        }

        // Without `names`, this is a value-lookup call, not functional enum creation.
        let names_arg = names_arg?;
        let spec = self.infer_enum_spec(names_arg, start, base_class, has_too_many_positional);

        let anchor = self.create_dynamic_enum_anchor(call_expr, definition, spec);
        let enum_lit = DynamicEnumLiteral::new(db, name, anchor, base_class, mixin_type);
        if let Err(error) = enum_lit.try_mro(db) {
            report_mro_error_kind(
                &self.context,
                error,
                enum_lit.name(db),
                call_expr,
                None,
                None,
            );
        }
        Some(Type::ClassLiteral(ClassLiteral::DynamicEnum(enum_lit)))
    }

    fn infer_enum_name_argument(
        &mut self,
        name_arg: &ast::Expr,
        base_class: KnownClass,
    ) -> Option<Name> {
        let db = self.db();
        let base_name = base_class.name(db);
        let name_type = self.expression_type(name_arg);

        let Some(name_literal) = name_type.as_string_literal() else {
            if !name_type.is_assignable_to(db, KnownClass::Str.to_instance(db))
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Invalid argument to parameter `value` of `{base_name}()`"
                ));
                diagnostic.set_primary_message(format_args!(
                    "Expected `str`, found `{}`",
                    name_type.display(db)
                ));
            }
            return None;
        };

        Some(Name::new(name_literal.value(db)))
    }

    fn infer_enum_start_argument(&mut self, value: &ast::Expr) -> i64 {
        let db = self.db();
        let ty = self.expression_type(value);
        if let Some(literal) = ty.as_int_literal() {
            return literal;
        }

        if !ty.is_assignable_to(db, KnownClass::Int.to_instance(db))
            && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, value)
        {
            builder.into_diagnostic(format_args!(
                "Expected `int` for `start` argument, got `{}`",
                ty.display(db),
            ));
        }

        1
    }

    fn infer_enum_mixin_argument(&mut self, value: &ast::Expr) -> Type<'db> {
        let db = self.db();
        let ty = self.expression_type(value);
        if let Some(class_lit) = ty.as_class_literal() {
            if class_lit.is_typed_dict(db)
                && let Some(builder) = self.context.report_lint(&INVALID_BASE, value)
            {
                builder.into_diagnostic(format_args!(
                    "TypedDict class `{}` cannot be used as an enum mixin",
                    ty.display(db),
                ));
            }
        } else if !ty.is_dynamic()
            && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, value)
        {
            builder.into_diagnostic(format_args!(
                "Expected a class for `type` argument, got `{}`",
                ty.display(db),
            ));
        }

        ty
    }

    fn infer_enum_spec(
        &mut self,
        names_arg: &ast::Expr,
        start: i64,
        base_class: KnownClass,
        has_too_many_positional: bool,
    ) -> EnumSpec<'db> {
        let db = self.db();
        let (members, has_known_members) = if has_too_many_positional {
            (vec![], false)
        } else {
            let (members, has_known_members) =
                self.parse_enum_members_arg(names_arg, start, base_class);
            if has_known_members && has_duplicate_enum_member_names(&members) {
                // Duplicate member names raise at runtime, so degrade to an unknown
                // member set and let normal call binding surface the rest.
                (vec![], false)
            } else {
                (members, has_known_members)
            }
        };

        EnumSpec::new(db, members.into_boxed_slice(), has_known_members)
    }

    fn create_dynamic_enum_anchor(
        &mut self,
        call_expr: &ast::ExprCall,
        definition: Option<Definition<'db>>,
        spec: EnumSpec<'db>,
    ) -> DynamicEnumAnchor<'db> {
        match definition {
            Some(definition) => DynamicEnumAnchor::Definition { definition, spec },
            None => {
                let db = self.db();
                let call_node_index = call_expr.node_index.load();
                let scope = self.scope();
                let scope_anchor = scope.node(db).node_index().unwrap_or(NodeIndex::from(0));
                let anchor_u32 = scope_anchor
                    .as_u32()
                    .expect("scope anchor should not be NodeIndex::NONE");
                let call_u32 = call_node_index
                    .as_u32()
                    .expect("call node should not be NodeIndex::NONE");
                DynamicEnumAnchor::ScopeOffset {
                    scope,
                    offset: call_u32 - anchor_u32,
                    spec,
                }
            }
        }
    }

    /// Parse the `names` argument of a functional enum call.
    ///
    /// Handles forms like:
    /// - `"RED GREEN BLUE"` (space/comma-separated string)
    /// - `["RED", "GREEN", "BLUE"]` (list of strings)
    /// - `[("RED", 1), ("GREEN", 2)]` (list of tuples)
    /// - `{"RED": 1, "GREEN": 2}` (dict mapping)
    fn parse_enum_members_arg(
        &mut self,
        names_arg: &ast::Expr,
        start: i64,
        base_class: KnownClass,
    ) -> (Vec<(Name, Type<'db>)>, bool) {
        let db = self.db();
        let ty = self.expression_type(names_arg);

        if let Some(string_lit) = ty.as_string_literal() {
            let s = string_lit.value(db);
            let names: Vec<Name> = s
                .split(|c: char| c == ',' || c.is_whitespace())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(Name::new)
                .collect();
            if names.is_empty() {
                return (vec![], false);
            }
            let members = names
                .into_iter()
                .enumerate()
                .map(|(i, n)| {
                    let v = enum_auto_value(db, base_class, n.as_str(), start, i);
                    (n, v)
                })
                .collect();
            return (members, true);
        }

        let elts = match names_arg {
            ast::Expr::List(list) => Some(list.elts.as_slice()),
            ast::Expr::Tuple(tup) => Some(tup.elts.as_slice()),
            _ => None,
        };
        if let Some(elts) = elts {
            return self.parse_enum_members_from_sequence(elts, start, base_class);
        }

        if let ast::Expr::Dict(dict) = names_arg {
            return self.parse_enum_members_from_dict(dict, start, base_class);
        }

        (vec![], false)
    }

    fn parse_enum_members_from_sequence(
        &mut self,
        elts: &[ast::Expr],
        start: i64,
        base_class: KnownClass,
    ) -> (Vec<(Name, Type<'db>)>, bool) {
        let db = self.db();
        let mut members = Vec::with_capacity(elts.len());
        let mut last_int_value: Option<i64> = start.checked_sub(1);
        for (i, elt) in elts.iter().enumerate() {
            let ty = self.expression_type(elt);
            if let Some(string_lit) = ty.as_string_literal() {
                let member_name = Name::new(string_lit.value(db));
                let v = enum_auto_value(db, base_class, member_name.as_str(), start, i);
                last_int_value = v.as_int_literal();
                members.push((member_name, v));
            } else if let Some((name, value)) = self.parse_explicit_enum_member(elt) {
                let value = if value.is_instance_of(db, KnownClass::Auto) {
                    next_auto_value(db, base_class, name.as_str(), last_int_value)
                } else {
                    value
                };
                last_int_value = value.as_int_literal();
                members.push((name, value));
            } else {
                return (vec![], false);
            }
        }
        (members, true)
    }

    /// Parse enum members from a dict literal like `{"RED": 1, "GREEN": 2}`.
    ///
    /// When `auto()` is used in a dict, CPython derives the next value from the
    /// previous member's value (not from `start + index`). For example,
    /// `Enum("E", {"A": 10, "B": auto()})` gives `B.value == 11`.
    fn parse_enum_members_from_dict(
        &mut self,
        dict: &ast::ExprDict,
        start: i64,
        base_class: KnownClass,
    ) -> (Vec<(Name, Type<'db>)>, bool) {
        let db = self.db();
        let mut members = Vec::with_capacity(dict.items.len());
        let mut last_int_value: Option<i64> = start.checked_sub(1);
        for item in &dict.items {
            let Some(key) = &item.key else {
                return (vec![], false);
            };
            let key_ty = self.expression_type(key);
            let Some(string_lit) = key_ty.as_string_literal() else {
                return (vec![], false);
            };
            let name = Name::new(string_lit.value(db));
            let raw_value = self.expression_type(&item.value);
            let value = if raw_value.is_instance_of(db, KnownClass::Auto) {
                next_auto_value(db, base_class, name.as_str(), last_int_value)
            } else {
                raw_value
            };
            last_int_value = value.as_int_literal();
            members.push((name, value));
        }
        if members.is_empty() {
            (vec![], false)
        } else {
            (members, true)
        }
    }

    /// Extract a `(name, value)` pair from a tuple element like `("RED", 1)`.
    fn parse_explicit_enum_member(&mut self, elt: &ast::Expr) -> Option<(Name, Type<'db>)> {
        let pair = match elt {
            ast::Expr::Tuple(tup) => &tup.elts,
            ast::Expr::List(list) => &list.elts,
            _ => return None,
        };
        let [name_expr, value_expr] = &**pair else {
            return None;
        };
        let db = self.db();
        let name_ty = self.expression_type(name_expr);
        let name = Name::new(name_ty.as_string_literal()?.value(db));
        let value = self.expression_type(value_expr);
        Some((name, value))
    }
}
