use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, NodeIndex, PythonVersion};
use rustc_hash::FxHashSet;

use crate::{
    Db, Program,
    semantic_index::definition::Definition,
    types::{
        ClassLiteral, KnownClass, Type, TypeContext, UnionType,
        class::{DynamicEnumAnchor, DynamicEnumLiteral, EnumSpec},
        constraints::ConstraintSetBuilder,
        diagnostic::{
            INVALID_ARGUMENT_TYPE, INVALID_BASE, PARAMETER_ALREADY_ASSIGNED,
            TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
        },
        infer::TypeInferenceBuilder,
        infer::builder::dynamic_class::report_mro_error_kind,
        subclass_of::SubclassOfType,
    },
};

#[derive(Copy, Clone, Debug)]
enum EnumStart {
    Literal(i64),
    DynamicInt,
}

/// Result of parsing the `names` argument of a functional enum call.
enum EnumMembersArgParseResult<'db> {
    /// The argument was parsed into a fully known member list.
    Known(KnownEnumMembers<'db>),
    /// The argument is valid, but some members are not known precisely.
    Unknown,
    /// The argument is definitely invalid for functional enum creation.
    Invalid,
}

/// Known members parsed from a functional enum `names` argument.
struct KnownEnumMembers<'db> {
    members: Vec<(Name, Type<'db>)>,
    value_form: EnumMemberValueForm,
}

/// Distinguishes whether member values are auto-generated from names or provided explicitly.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum EnumMemberValueForm {
    /// Values are derived from member names via the enum's implicit auto-value rules.
    Generated,
    /// Values are supplied directly by the functional enum input.
    Explicit,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum TypeMixinMemberBehavior {
    /// Preserve both member names and values precisely.
    Precise,
    /// Convert generated values through a known builtin mixin, widening when exact literals are
    /// not representable or would be impractical to materialize.
    ConvertedValues,
    /// Hide the member set entirely because the mixin may change canonical members or aliasing.
    UnknownMembers,
}

impl TypeMixinMemberBehavior {
    /// Classifies how much member information can be preserved for a functional enum with `type=`.
    ///
    /// Generated-name forms can keep more information for a small set of builtin mixins whose value
    /// conversion rules we model explicitly. Explicit-value forms are treated more conservatively,
    /// since applying the mixin can change aliasing and therefore the canonical member set.
    fn from_mixin(db: &dyn Db, mixin_type: Type<'_>, value_form: EnumMemberValueForm) -> Self {
        match value_form {
            EnumMemberValueForm::Explicit => Self::UnknownMembers,
            EnumMemberValueForm::Generated => match mixin_type {
                Type::ClassLiteral(ClassLiteral::Static(class)) => match class.known(db) {
                    Some(KnownClass::Int) => Self::Precise,
                    Some(KnownClass::Str | KnownClass::Bytes | KnownClass::Float) => {
                        Self::ConvertedValues
                    }
                    _ => Self::UnknownMembers,
                },
                _ => Self::UnknownMembers,
            },
        }
    }
}

/// Classification of one element in a sequence-form `names` argument.
enum SequenceEnumMember<'db> {
    /// A known string member name like `"RED"`.
    NameKnown(Name),
    /// A name entry whose type is compatible with `str`, but not precise.
    NameOpaque,
    /// A known `(name, value)` pair like `("RED", 1)`.
    PairKnown(Name, Type<'db>),
    /// A potential `(name, value)` pair whose name position is not precise.
    PairOpaque,
    /// An element that cannot participate in functional enum member parsing.
    Invalid,
}

/// Distinguishes whether a sequence-form `names` argument uses bare names or explicit pairs.
#[derive(Copy, Clone)]
enum SequenceEnumMemberForm {
    /// The sequence is a list of member names.
    Names,
    /// The sequence is a list of explicit `(name, value)` pairs.
    Pairs,
}

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

/// Returns the effective `_EnumNames` type accepted by the functional enum APIs.
///
/// This includes the string form, iterables of strings, iterables of
/// iterable-like `(name, value)` pairs, and mappings from `str` to values.
fn enum_names_type(db: &dyn Db) -> Type<'_> {
    let str_type = KnownClass::Str.to_instance(db);
    let iterable_str = KnownClass::Iterable.to_specialized_instance(db, &[str_type]);
    let iterable_object = KnownClass::Iterable.to_specialized_instance(db, &[Type::object()]);
    let iterable_iterable_object =
        KnownClass::Iterable.to_specialized_instance(db, &[iterable_object]);
    let mapping_str_object = KnownClass::Mapping
        .to_specialized_instance(db, &[KnownClass::Str.to_instance(db), Type::object()]);
    UnionType::from_elements(
        db,
        [
            str_type,
            iterable_str,
            iterable_iterable_object,
            mapping_str_object,
        ],
    )
}

/// Compute the first auto-assigned value for functional enum forms that only specify names.
///
/// `StrEnum` ignores `start` and uses the lowercased member name. Other enum kinds use the
/// literal `start` value when available, and widen to `int` when `start` is a non-literal int.
fn first_enum_auto_value<'db>(
    db: &'db dyn Db,
    base_class: KnownClass,
    name: &str,
    start: EnumStart,
) -> Type<'db> {
    match base_class {
        KnownClass::StrEnum => Type::string_literal(db, &name.to_lowercase()),
        _ => match start {
            EnumStart::Literal(start) => Type::int_literal(start),
            EnumStart::DynamicInt => KnownClass::Int.to_instance(db),
        },
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
                        1_u64
                            .checked_shl(shift)
                            .and_then(|value| i64::try_from(value).ok())
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

fn enum_members_from_names(
    db: &dyn Db,
    names: Vec<Name>,
    start: EnumStart,
    base_class: KnownClass,
) -> Vec<(Name, Type<'_>)> {
    let mut members = Vec::with_capacity(names.len());
    let mut last_int_value = None;

    for (index, name) in names.into_iter().enumerate() {
        let value = if index == 0 {
            first_enum_auto_value(db, base_class, name.as_str(), start)
        } else {
            next_auto_value(db, base_class, name.as_str(), last_int_value)
        };
        last_int_value = value.as_int_literal();
        members.push((name, value));
    }

    members
}

/// Converts generated functional-enum member values through a known builtin `type=` mixin.
///
/// This preserves exact `str` literals when the generated auto-values are known integer literals,
/// otherwise preserving at least the builtin result type for `str`, `bytes`, and `float`.
///
/// Returns `None` when the mixin is not a supported builtin or when the generated values are not
/// compatible with the corresponding builtin conversion.
fn apply_generated_type_mixin_member_values<'db>(
    db: &'db dyn Db,
    mixin_type: Type<'_>,
    members: Vec<(Name, Type<'db>)>,
) -> Option<Vec<(Name, Type<'db>)>> {
    let Type::ClassLiteral(ClassLiteral::Static(class)) = mixin_type else {
        return None;
    };

    match class.known(db) {
        Some(KnownClass::Str) => Some(
            members
                .into_iter()
                .map(|(name, value)| {
                    let value = if let Some(literal) = value.as_int_literal() {
                        Type::string_literal(db, &literal.to_string())
                    } else if value.is_assignable_to(db, KnownClass::Int.to_instance(db)) {
                        KnownClass::Str.to_instance(db)
                    } else {
                        return None;
                    };
                    Some((name, value))
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        Some(KnownClass::Bytes) => Some(
            members
                .into_iter()
                .map(|(name, value)| {
                    let value = if value.is_assignable_to(db, KnownClass::Int.to_instance(db)) {
                        KnownClass::Bytes.to_instance(db)
                    } else {
                        return None;
                    };
                    Some((name, value))
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        Some(KnownClass::Float) => Some(
            members
                .into_iter()
                .map(|(name, value)| {
                    if value.is_assignable_to(db, KnownClass::Int.to_instance(db)) {
                        Some((name, KnownClass::Float.to_instance(db)))
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => None,
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

        let has_positional_keyword_conflict =
            (!args.is_empty() && value_kw.is_some()) || (args.len() >= 2 && names_kw.is_some());
        if !args.is_empty()
            && let Some(keyword) = value_kw
            && let Some(builder) = self
                .context
                .report_lint(&PARAMETER_ALREADY_ASSIGNED, keyword)
        {
            builder.into_diagnostic(format_args!(
                "Multiple values provided for parameter `value` of `{base_name}()`"
            ));
        }
        if args.len() >= 2
            && let Some(keyword) = names_kw
            && let Some(builder) = self
                .context
                .report_lint(&PARAMETER_ALREADY_ASSIGNED, keyword)
        {
            builder.into_diagnostic(format_args!(
                "Multiple values provided for parameter `names` of `{base_name}()`"
            ));
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

        let start = start_kw.map_or(EnumStart::Literal(1), |kw| {
            self.infer_enum_start_argument(&kw.value)
        });
        let (mixin_type, valid_mixin_type) = type_kw.map_or((None, true), |kw| {
            self.infer_enum_mixin_argument(&kw.value, base_class)
        });

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
        let spec = self.infer_enum_spec(
            names_arg,
            start,
            base_class,
            mixin_type,
            has_too_many_positional || has_positional_keyword_conflict || !valid_mixin_type,
        );

        // Non-literal names use the ordinary `type[EnumSubclass]` overload result
        // instead of synthesizing a `DynamicEnumLiteral`.
        let Some(name) = self.infer_enum_name_argument(name_arg, base_class) else {
            return SubclassOfType::try_from_type(db, base_class.to_class_literal(db));
        };

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

    fn infer_enum_start_argument(&mut self, value: &ast::Expr) -> EnumStart {
        let db = self.db();
        let ty = self.expression_type(value);
        if let Some(literal) = ty.as_int_literal() {
            return EnumStart::Literal(literal);
        }

        if ty.is_assignable_to(db, KnownClass::Int.to_instance(db)) {
            return EnumStart::DynamicInt;
        }

        if let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, value) {
            builder.into_diagnostic(format_args!(
                "Expected `int` for `start` argument, got `{}`",
                ty.display(db),
            ));
        }

        EnumStart::Literal(1)
    }

    fn infer_enum_mixin_argument(
        &mut self,
        value: &ast::Expr,
        base_class: KnownClass,
    ) -> (Option<Type<'db>>, bool) {
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
                return (None, false);
            }

            let Some(mixin_class) = ty.to_class_type(db) else {
                return (Some(ty), true);
            };
            let Some(enum_base) = base_class.to_class_literal(db).to_class_type(db) else {
                return (Some(ty), true);
            };
            let constraints = ConstraintSetBuilder::new();
            if !mixin_class.could_coexist_in_mro_with(db, enum_base, &constraints)
                && let Some(builder) = self.context.report_lint(&INVALID_BASE, value)
            {
                builder.into_diagnostic(format_args!(
                    "Class `{}` cannot be used as an enum mixin with `{}`",
                    mixin_class.name(db),
                    base_class.name(db),
                ));
                return (None, false);
            }
            return (Some(ty), true);
        }

        if ty.is_dynamic() {
            return (Some(ty), true);
        }

        if let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, value) {
            builder.into_diagnostic(format_args!(
                "Expected a class for `type` argument, got `{}`",
                ty.display(db),
            ));
        }

        (None, false)
    }

    fn infer_enum_spec(
        &mut self,
        names_arg: &ast::Expr,
        start: EnumStart,
        base_class: KnownClass,
        mixin_type: Option<Type<'db>>,
        has_invalid_arguments: bool,
    ) -> EnumSpec<'db> {
        let db = self.db();
        let (members, has_known_members) = if has_invalid_arguments {
            (vec![], false)
        } else {
            match self.parse_enum_members_arg(names_arg, start, base_class) {
                EnumMembersArgParseResult::Known(known_members) => {
                    let mut seen = FxHashSet::default();
                    if known_members
                        .members
                        .iter()
                        .any(|(name, _)| !seen.insert(name.clone()))
                    {
                        // Duplicate member names raise at runtime, so degrade to an unknown
                        // member set and let normal call binding surface the rest.
                        (vec![], false)
                    } else if let Some(mixin_type) = mixin_type {
                        match TypeMixinMemberBehavior::from_mixin(
                            db,
                            mixin_type,
                            known_members.value_form,
                        ) {
                            TypeMixinMemberBehavior::Precise => (known_members.members, true),
                            TypeMixinMemberBehavior::ConvertedValues => {
                                match apply_generated_type_mixin_member_values(
                                    db,
                                    mixin_type,
                                    known_members.members,
                                ) {
                                    Some(members) => (members, true),
                                    None => (vec![], false),
                                }
                            }
                            // `type=` can change aliasing and resulting values, so when the mixin
                            // semantics are not predictable we avoid exposing a precise member set.
                            TypeMixinMemberBehavior::UnknownMembers => (vec![], false),
                        }
                    } else {
                        (known_members.members, true)
                    }
                }
                EnumMembersArgParseResult::Unknown => (vec![], false),
                EnumMembersArgParseResult::Invalid => {
                    self.report_invalid_enum_names_argument(names_arg, base_class);
                    (vec![], false)
                }
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
        start: EnumStart,
        base_class: KnownClass,
    ) -> EnumMembersArgParseResult<'db> {
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
            let members = enum_members_from_names(db, names, start, base_class);
            return EnumMembersArgParseResult::Known(KnownEnumMembers {
                members,
                value_form: EnumMemberValueForm::Generated,
            });
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
            return self.parse_enum_members_from_dict(dict, base_class);
        }

        if ty.is_dynamic() || ty.is_assignable_to(db, enum_names_type(db)) {
            EnumMembersArgParseResult::Unknown
        } else {
            EnumMembersArgParseResult::Invalid
        }
    }

    fn parse_enum_members_from_sequence(
        &mut self,
        elts: &[ast::Expr],
        start: EnumStart,
        base_class: KnownClass,
    ) -> EnumMembersArgParseResult<'db> {
        let db = self.db();
        let mut names = Vec::with_capacity(elts.len());
        let mut explicit_members = Vec::with_capacity(elts.len());
        let mut form = None;
        let mut has_opaque_members = false;

        for elt in elts {
            if matches!(elt, ast::Expr::Starred(_)) {
                return EnumMembersArgParseResult::Invalid;
            }
            match self.classify_sequence_enum_member(elt) {
                SequenceEnumMember::NameKnown(name) => {
                    if matches!(form, Some(SequenceEnumMemberForm::Pairs)) {
                        return EnumMembersArgParseResult::Invalid;
                    }
                    form = Some(SequenceEnumMemberForm::Names);
                    names.push(name);
                }
                SequenceEnumMember::NameOpaque => {
                    if matches!(form, Some(SequenceEnumMemberForm::Pairs)) {
                        return EnumMembersArgParseResult::Invalid;
                    }
                    form = Some(SequenceEnumMemberForm::Names);
                    has_opaque_members = true;
                }
                SequenceEnumMember::PairKnown(name, value) => {
                    if matches!(form, Some(SequenceEnumMemberForm::Names)) {
                        return EnumMembersArgParseResult::Invalid;
                    }
                    form = Some(SequenceEnumMemberForm::Pairs);
                    explicit_members.push((name, value));
                }
                SequenceEnumMember::PairOpaque => {
                    if matches!(form, Some(SequenceEnumMemberForm::Names)) {
                        return EnumMembersArgParseResult::Invalid;
                    }
                    form = Some(SequenceEnumMemberForm::Pairs);
                    has_opaque_members = true;
                }
                SequenceEnumMember::Invalid => return EnumMembersArgParseResult::Invalid,
            }
        }

        if has_opaque_members {
            return EnumMembersArgParseResult::Unknown;
        }

        if matches!(form, Some(SequenceEnumMemberForm::Names)) {
            return EnumMembersArgParseResult::Known(KnownEnumMembers {
                members: enum_members_from_names(db, names, start, base_class),
                value_form: EnumMemberValueForm::Generated,
            });
        }
        if form.is_none() {
            return EnumMembersArgParseResult::Known(KnownEnumMembers {
                members: vec![],
                value_form: EnumMemberValueForm::Generated,
            });
        }

        let mut members = Vec::with_capacity(elts.len());
        // Explicit-value forms match static enums: `start` is ignored and a leading `auto()`
        // still begins from the default seed of `1`.
        let mut last_int_value = Some(0);
        for (name, value) in explicit_members {
            let value = if value.is_instance_of(db, KnownClass::Auto) {
                next_auto_value(db, base_class, name.as_str(), last_int_value)
            } else {
                value
            };
            last_int_value = value.as_int_like_literal();
            members.push((name, value));
        }
        EnumMembersArgParseResult::Known(KnownEnumMembers {
            members,
            value_form: EnumMemberValueForm::Explicit,
        })
    }

    /// Parse enum members from a dict literal like `{"RED": 1, "GREEN": 2}`.
    ///
    /// When `auto()` is used in a dict, CPython derives the next value from the
    /// previous member's value (not from `start + index`). For example,
    /// `Enum("E", {"A": 10, "B": auto()})` gives `B.value == 11`.
    fn parse_enum_members_from_dict(
        &mut self,
        dict: &ast::ExprDict,
        base_class: KnownClass,
    ) -> EnumMembersArgParseResult<'db> {
        let db = self.db();
        let mut members = Vec::with_capacity(dict.items.len());
        let mut last_int_value = Some(0);
        let mut has_opaque_keys = false;
        for item in &dict.items {
            let Some(key) = &item.key else {
                return EnumMembersArgParseResult::Invalid;
            };
            let key_ty = self.expression_type(key);
            let Some(string_lit) = key_ty.as_string_literal() else {
                if key_ty.is_dynamic()
                    || key_ty.is_assignable_to(db, KnownClass::Str.to_instance(db))
                {
                    has_opaque_keys = true;
                    continue;
                }
                return EnumMembersArgParseResult::Invalid;
            };
            let name = Name::new(string_lit.value(db));
            let raw_value = self.expression_type(&item.value);
            let value = if raw_value.is_instance_of(db, KnownClass::Auto) {
                next_auto_value(db, base_class, name.as_str(), last_int_value)
            } else {
                raw_value
            };
            last_int_value = value.as_int_like_literal();
            members.push((name, value));
        }
        if has_opaque_keys {
            EnumMembersArgParseResult::Unknown
        } else {
            EnumMembersArgParseResult::Known(KnownEnumMembers {
                members,
                value_form: EnumMemberValueForm::Explicit,
            })
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

    /// Returns `true` if `elt` could be an explicit `(name, value)` member pair.
    ///
    /// This is used when the name position is not a known string literal, but
    /// is still compatible with `str`.
    fn is_potential_explicit_enum_member(&mut self, elt: &ast::Expr) -> bool {
        let pair = match elt {
            ast::Expr::Tuple(tup) => &tup.elts,
            ast::Expr::List(list) => &list.elts,
            _ => return false,
        };
        let [name_expr, _value_expr] = &**pair else {
            return false;
        };
        let db = self.db();
        let name_ty = self.expression_type(name_expr);
        name_ty.is_dynamic() || name_ty.is_assignable_to(db, KnownClass::Str.to_instance(db))
    }

    /// Classifies one element from a sequence-form `names` argument.
    ///
    /// This distinguishes between known names, opaque names, known explicit
    /// pairs, opaque explicit pairs, and definitely invalid elements.
    fn classify_sequence_enum_member(&mut self, elt: &ast::Expr) -> SequenceEnumMember<'db> {
        let db = self.db();
        let ty = self.expression_type(elt);
        if let Some(string_lit) = ty.as_string_literal() {
            return SequenceEnumMember::NameKnown(Name::new(string_lit.value(db)));
        }
        if let Some((name, value)) = self.parse_explicit_enum_member(elt) {
            return SequenceEnumMember::PairKnown(name, value);
        }
        if ty.is_dynamic() || ty.is_assignable_to(db, KnownClass::Str.to_instance(db)) {
            return SequenceEnumMember::NameOpaque;
        }
        if self.is_potential_explicit_enum_member(elt) {
            return SequenceEnumMember::PairOpaque;
        }
        SequenceEnumMember::Invalid
    }

    fn report_invalid_enum_names_argument(
        &mut self,
        names_arg: &ast::Expr,
        base_class: KnownClass,
    ) {
        let db = self.db();
        let base_name = base_class.name(db);
        let names_ty = self.expression_type(names_arg);
        if let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, names_arg) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Invalid argument to parameter `names` of `{base_name}()`"
            ));
            diagnostic.set_primary_message(format_args!(
                "Expected `{}`, found `{}`",
                enum_names_type(db).display(db),
                names_ty.display(db),
            ));
        }
    }
}
