use itertools::Itertools;
use ruff_python_ast::{self as ast, HasNodeIndex};
use rustc_hash::FxHashMap;

use super::{ArgExpr, TypeInferenceBuilder};
use crate::types::typed_dict::validate_typed_dict_constructor;
use crate::types::{KnownClass, Type, TypeContext};

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn infer_keyword_only_dict_call(
        &mut self,
        func: &ast::Expr,
        arguments: &ast::Arguments,
        call_expression_tcx: TypeContext<'db>,
    ) -> Option<Type<'db>> {
        if !arguments.args.is_empty()
            || arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
        {
            return None;
        }

        // Fast-path dict(...) in TypedDict context: infer keyword values against fields,
        // then validate and return the TypedDict type.
        if let Some(tcx) = call_expression_tcx.annotation
            && let Some(typed_dict) = tcx
                .filter_union(self.db(), Type::is_typed_dict)
                .as_typed_dict()
        {
            let items = typed_dict.items(self.db());
            for keyword in &arguments.keywords {
                if let Some(arg_name) = &keyword.arg {
                    let value_tcx = items
                        .get(arg_name.id.as_str())
                        .map(|field| TypeContext::new(Some(field.declared_ty)))
                        .unwrap_or_default();
                    self.infer_expression(&keyword.value, value_tcx);
                }
            }

            validate_typed_dict_constructor(
                &self.context,
                typed_dict,
                arguments,
                func.into(),
                |expr, _| self.expression_type(expr),
            );

            return Some(Type::TypedDict(typed_dict));
        }

        // Lower `dict(a=..., b=...)` to synthetic `(Literal["a"], value)` pairs so we can
        // reuse dict-literal inference. We key the synthetic name off the value node because
        // `infer_collection_literal` operates on expressions rather than keywords.
        let items = arguments
            .keywords
            .iter()
            .map(|keyword| [Some(&keyword.value), Some(&keyword.value)])
            .collect_vec();
        let keyword_names = arguments
            .keywords
            .iter()
            .filter_map(|keyword| {
                Some((
                    keyword.value.node_index().load(),
                    keyword.arg.as_ref()?.id.clone(),
                ))
            })
            .collect::<FxHashMap<_, _>>();
        let mut infer_elt_ty = |builder: &mut Self, (i, elt, tcx): ArgExpr<'db, '_>| {
            if i == 0 {
                let key = keyword_names
                    .get(&elt.node_index().load())
                    .expect("keyword-only dict() fast-path requires named keywords");
                Type::string_literal(builder.db(), key.as_str())
            } else {
                builder.infer_expression(elt, tcx)
            }
        };

        self.infer_collection_literal(
            None,
            KnownClass::Dict,
            &items,
            &mut infer_elt_ty,
            call_expression_tcx,
        )
    }
}
