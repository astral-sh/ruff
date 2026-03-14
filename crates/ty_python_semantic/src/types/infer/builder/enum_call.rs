use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, NodeIndex};

use crate::{
    Db,
    semantic_index::definition::Definition,
    types::{
        ClassLiteral, KnownClass, Type, TypeContext,
        class::{DynamicEnumAnchor, DynamicEnumLiteral, EnumSpec},
        infer::TypeInferenceBuilder,
    },
};

/// Find enum bass class of the given type.
pub(crate) fn enum_functional_call_base<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<KnownClass> {
    let ClassLiteral::Static(cls) = ty.as_class_literal()? else {
        return None;
    };
    cls.known(db)
        .filter(|k| matches!(k, KnownClass::Enum | KnownClass::StrEnum))
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer a `Enum("Color", "RED", "GREEN", "BLUE")` call.
    pub(crate) fn infer_enum_call_expression(
        &mut self,
        call_expr: &ast::ExprCall,
        definition: Option<Definition<'db>>,
    ) -> Type<'db> {
        let db = self.db();
        let args = &call_expr.arguments.args;

        let name_type = self.infer_expression(&args[0], TypeContext::default());
        let name = if let Some(lit) = name_type.as_string_literal() {
            Name::new(lit.value(db))
        } else {
            Name::new_static("<unknown>")
        };

        let extra_args = &args[1..];
        let (spec, has_known_members) = self.parse_enum_members(extra_args);

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

        let enum_lit = DynamicEnumLiteral::new(db, name, anchor);
        Type::ClassLiteral(ClassLiteral::DynamicEnum(enum_lit))
    }

    /// Extract members from additional arguments.
    ///
    /// These can come in many forms. Examples:
    /// - "RED", "GREEN", "BLUE")
    /// - "RED GREEN BLUE"
    /// - [("RED", 1), ("GREEN", 2), ("BLUE", 3)]
    fn parse_enum_members(&mut self, extra_args: &[ast::Expr]) -> (Vec<(Name, Type<'db>)>, bool) {
        let db = self.db();

        if extra_args.len() == 1 {
            let arg = &extra_args[0];
            let ty = self.infer_expression(arg, TypeContext::default());

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
                        let v = Type::int_literal(i64::try_from(i + 1).unwrap_or(1));
                        (n, v)
                    })
                    .collect();
                return (members, true);
            }

            let elts = match arg {
                ast::Expr::List(list) => Some(list.elts.as_slice()),
                ast::Expr::Tuple(tup) => Some(tup.elts.as_slice()),
                _ => None,
            };
            if let Some(elts) = elts {
                return self.parse_enum_members_from_already_inferred(elts);
            }

            return (vec![], false);
        }

        let mut members = Vec::with_capacity(extra_args.len());
        for (i, arg) in extra_args.iter().enumerate() {
            let ty = self.infer_expression(arg, TypeContext::default());
            if let Some(string_lit) = ty.as_string_literal() {
                let member_name = Name::new(string_lit.value(db));
                let v = Type::int_literal(i64::try_from(i + 1).unwrap_or(1));
                members.push((member_name, v));
            } else {
                return (vec![], false);
            }
        }
        (members, true)
    }

    fn parse_enum_members_from_already_inferred(
        &mut self,
        elts: &[ast::Expr],
    ) -> (Vec<(Name, Type<'db>)>, bool) {
        let db = self.db();
        let mut members = Vec::with_capacity(elts.len());
        for (i, elt) in elts.iter().enumerate() {
            let ty = self.expression_type(elt);
            if let Some(string_lit) = ty.as_string_literal() {
                let member_name = Name::new(string_lit.value(db));
                let v = Type::int_literal(i64::try_from(i + 1).unwrap_or(1));
                members.push((member_name, v));
            } else {
                return (vec![], false);
            }
        }
        (members, true)
    }
}
