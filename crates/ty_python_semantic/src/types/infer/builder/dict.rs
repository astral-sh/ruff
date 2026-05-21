use itertools::Itertools;
use ruff_python_ast::{self as ast, HasNodeIndex};
use rustc_hash::FxHashMap;

use super::{ArgExpr, TypeInferenceBuilder};
use crate::types::typed_dict::{
    extract_unpacked_typed_dict_keys_from_value_type, infer_unpacked_keyword_types,
    validate_typed_dict_constructor,
};
use crate::types::{KnownClass, Type, TypeContext};

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn infer_keyword_only_dict_call(
        &mut self,
        func: &ast::Expr,
        arguments: &ast::Arguments,
        call_expression_tcx: TypeContext<'db>,
    ) -> Option<Type<'db>> {
        if !arguments.args.is_empty() {
            return None;
        }

        // Fast-path dict(...) in TypedDict context: infer keyword values against fields,
        // then validate and return the TypedDict type. This also covers `dict(**src)` when `src`
        // is `TypedDict`-shaped.
        if let Some(tcx) = call_expression_tcx.annotation
            && let Some(typed_dict) = tcx
                .filter_union(self.db(), Type::is_typed_dict)
                .as_typed_dict()
        {
            // Only speculate the `**kwargs` applicability check. Assignability handles inputs that
            // are already valid for the target, including gradual and bottom types. The additional
            // TypedDict-shape check keeps invalid-but-analyzable unpacks on this path so validation
            // can emit key-level diagnostics instead of falling back to a broad `dict[...]`
            // assignment error. Unsupported unpacks still fall back to ordinary `dict(...)`
            // inference.
            //
            // Named keyword values are inferred on the real builder so their diagnostics are either
            // committed with the fast path or left for ordinary `dict(...)` inference when we fall
            // back.
            let supports_typed_dict_context = {
                let mut speculative_builder = self.speculate();
                infer_unpacked_keyword_types(arguments, |expr, tcx| {
                    speculative_builder.infer_expression(expr, tcx)
                })
                .into_iter()
                .flatten()
                .all(|keyword_ty| {
                    keyword_ty
                        .is_assignable_to(speculative_builder.db(), Type::TypedDict(typed_dict))
                        || extract_unpacked_typed_dict_keys_from_value_type(
                            speculative_builder.db(),
                            keyword_ty,
                        )
                        .is_some()
                })
            };

            if supports_typed_dict_context {
                self.infer_typed_dict_constructor_keyword_values(typed_dict, arguments);
                validate_typed_dict_constructor(
                    &self.context,
                    typed_dict,
                    arguments,
                    func.into(),
                    |expr, _| self.expression_type(expr),
                );

                return Some(Type::TypedDict(typed_dict));
            }
        }

        if arguments
            .keywords
            .iter()
            .any(|keyword| keyword.arg.is_none())
        {
            return None;
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
            KnownClass::Dict,
            &items,
            &mut infer_elt_ty,
            call_expression_tcx,
        )
    }
}
