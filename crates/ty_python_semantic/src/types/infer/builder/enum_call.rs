use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, NodeIndex};

use crate::{
    Db,
    semantic_index::definition::Definition,
    types::{
        ClassLiteral, KnownClass, Type, TypeContext,
        class::{DynamicEnumAnchor, DynamicEnumLiteral, EnumSpec},
        infer::TypeInferenceBuilder,
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
            Type::int_literal(start << i64::try_from(index).unwrap_or(0))
        }
        _ => Type::int_literal(start + i64::try_from(index).unwrap_or(0)),
    }
}

/// Compute the next auto-assigned value for a dict-form enum member,
/// based on the previous member's value rather than positional index.
///
/// This matches CPython's `_generate_next_value_` behavior for dict form:
/// - `StrEnum`: lowercased member name
/// - `Flag`/`IntFlag`: next highest power of two
/// - Others: `last_value + 1`
fn dict_auto_value<'db>(
    db: &'db dyn Db,
    base_class: KnownClass,
    name: &str,
    last_int_value: i64,
) -> Type<'db> {
    match base_class {
        KnownClass::StrEnum => Type::string_literal(db, &name.to_lowercase()),
        KnownClass::Flag | KnownClass::IntFlag => {
            // next power of two after highest bit in the last value
            if last_int_value <= 0 {
                Type::int_literal(1)
            } else {
                Type::int_literal(1 << (i64::BITS - last_int_value.leading_zeros()))
            }
        }
        _ => Type::int_literal(last_int_value + 1),
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
        let args = &call_expr.arguments.args;
        let keywords = &call_expr.arguments.keywords;

        // bail out on unknown keywords so normal overload resolution can diagnose them
        let has_unknown_keyword = keywords.iter().any(|kw| {
            kw.arg.as_ref().is_some_and(|name| {
                !matches!(
                    name.as_str(),
                    "value" | "names" | "start" | "type" | "module" | "qualname" | "boundary"
                )
            })
        });
        if has_unknown_keyword {
            return None;
        }

        let value_kw = call_expr.arguments.find_keyword("value");
        let names_kw = call_expr.arguments.find_keyword("names");
        let start_kw = call_expr.arguments.find_keyword("start");
        let type_kw = call_expr.arguments.find_keyword("type");

        // bail out on positional/keyword conflicts so normal binding can diagnose them
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

        // Infer all positional arg types.
        for arg in args {
            self.infer_expression(arg, TypeContext::default());
        }
        for kw in keywords {
            self.infer_expression(&kw.value, TypeContext::default());
        }

        // Non-literal name: return type[base_class] without creating a
        // DynamicEnumLiteral. This matches the typeshed overload return type.
        if !name_arg.is_string_literal_expr() {
            return SubclassOfType::try_from_type(db, base_class.to_class_literal(db));
        }

        let start = start_kw
            .and_then(|kw| self.expression_type(&kw.value).as_int_literal())
            .unwrap_or(1);

        let mixin_type = type_kw.map(|kw| self.expression_type(&kw.value));

        let name_type = self.expression_type(name_arg);
        let name = Name::new(
            name_type
                .as_string_literal()
                .expect("name arg should be a string literal")
                .value(db),
        );

        // Only 1 extra positional arg is allowed (the `names` parameter).
        // `Enum("Color", "RED", "GREEN")` is invalid at runtime.
        let has_too_many_positional = args.len() > 2;

        // without `names`, this is a value-lookup call, not functional enum creation
        let names_arg = names_arg?;

        let (spec, has_known_members) = if has_too_many_positional {
            (vec![], false)
        } else {
            let (spec, has_known_members) = self.parse_enum_names(names_arg, start, base_class);
            // degrade on duplicate member names (runtime raises TypeError)
            let has_duplicates = has_known_members
                && spec
                    .iter()
                    .enumerate()
                    .any(|(i, (name, _))| spec[..i].iter().any(|(prev, _)| prev == name));
            if has_duplicates {
                (vec![], false)
            } else {
                (spec, has_known_members)
            }
        };

        let spec = EnumSpec::new(db, spec.into_boxed_slice(), has_known_members);

        let anchor = match definition {
            Some(def) => DynamicEnumAnchor::Definition {
                definition: def,
                spec,
            },
            None => {
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
        };

        let enum_lit = DynamicEnumLiteral::new(db, name, anchor, base_class, mixin_type);
        Some(Type::ClassLiteral(ClassLiteral::DynamicEnum(enum_lit)))
    }

    /// Parse the `names` argument of a functional enum call.
    ///
    /// Handles forms like:
    /// - `"RED GREEN BLUE"` (space/comma-separated string)
    /// - `["RED", "GREEN", "BLUE"]` (list of strings)
    /// - `[("RED", 1), ("GREEN", 2)]` (list of tuples)
    /// - `{"RED": 1, "GREEN": 2}` (dict mapping)
    fn parse_enum_names(
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
            return self.parse_enum_members_from_already_inferred(elts, start, base_class);
        }

        if let ast::Expr::Dict(dict) = names_arg {
            return self.parse_enum_members_from_dict(dict, start, base_class);
        }

        (vec![], false)
    }

    fn parse_enum_members_from_already_inferred(
        &mut self,
        elts: &[ast::Expr],
        start: i64,
        base_class: KnownClass,
    ) -> (Vec<(Name, Type<'db>)>, bool) {
        let db = self.db();
        let mut members = Vec::with_capacity(elts.len());
        for (i, elt) in elts.iter().enumerate() {
            let ty = self.expression_type(elt);
            if let Some(string_lit) = ty.as_string_literal() {
                let member_name = Name::new(string_lit.value(db));
                let v = enum_auto_value(db, base_class, member_name.as_str(), start, i);
                members.push((member_name, v));
            } else if let Some((name, value)) = self.extract_enum_tuple_entry(elt) {
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
        let mut last_int_value = start - 1;
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
                dict_auto_value(db, base_class, name.as_str(), last_int_value)
            } else {
                raw_value
            };
            if let Some(int_val) = value.as_int_literal() {
                last_int_value = int_val;
            }
            members.push((name, value));
        }
        if members.is_empty() {
            (vec![], false)
        } else {
            (members, true)
        }
    }

    /// Extract a `(name, value)` pair from a tuple element like `("RED", 1)`.
    fn extract_enum_tuple_entry(&mut self, elt: &ast::Expr) -> Option<(Name, Type<'db>)> {
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
