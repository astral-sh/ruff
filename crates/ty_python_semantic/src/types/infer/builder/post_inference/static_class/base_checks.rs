//! Shared ordering and retained state for validation of a class's explicit bases.

use std::convert::Infallible;

use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::{SemanticIndex, definition::Definition};

use crate::types::class::{
    CodeGeneratorKind, DisjointBase, ExpandedClassBaseEntry, expanded_class_base_entries,
};
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    INVALID_BASE, INVALID_GENERIC_CLASS, INVALID_NAMED_TUPLE, IncompatibleBases,
    SUBCLASS_OF_DATACLASS_WITH_ORDER, SUBCLASS_OF_FINAL_CLASS,
    report_bad_frozen_dataclass_inheritance, report_missing_type_arguments,
    report_unsupported_base,
};
use crate::types::generics::GenericContext;
use crate::types::tuple::Tuple;
use crate::types::variance::VarianceInferable;
use crate::types::{
    ClassType, GenericAlias, KnownInstanceType, SpecialFormType, StaticClassLiteral, Type,
    TypeVarVariance, definition_expression_type,
};

use super::phases::StaticClassBaseChecks;

#[cfg(test)]
mod tests;

pub(super) struct OrdinaryExplicitBaseCheckEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
    pub(super) index: &'a SemanticIndex<'db>,
}

pub(in crate::types::infer::builder) struct ExplicitBaseCheckFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousExplicitBaseCheckEffects)]
    pub(in crate::types::infer::builder) trait ExplicitBaseCheckEffects<'db> {
        type Error;

        #[operation(local)]
        async fn empty_disjoint_bases(&self) -> Result<IncompatibleBases<'db>, Self::Error>;
        #[operation(local)]
        async fn empty_typed_dict_bases(&self) -> Result<Vec<ClassType<'db>>, Self::Error>;
        #[operation(local)]
        async fn class_definition(&self, class_node: &ast::StmtClassDef) -> Result<Definition<'db>, Self::Error>;
        #[operation(child)]
        async fn expand<'node>(&self, class: StaticClassLiteral<'db>, class_node: &'node ast::StmtClassDef, definition: Definition<'db>) -> Result<Vec<ExpandedClassBaseEntry<'node, 'db>>, Self::Error>;
        #[operation(local)]
        async fn explicit_variance_enabled(&self) -> Result<bool, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_entry<'node>(&self, entries: &[ExpandedClassBaseEntry<'node, 'db>], cursor: &mut usize) -> Result<Option<(usize, ExpandedClassBaseEntry<'node, 'db>)>, Self::Error>;
        #[operation(child)]
        async fn report_missing_arguments(&self, base: Type<'db>, source_node: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_named_tuple(&self, class: StaticClassLiteral<'db>, source_node: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_plain_generic(&self, source_node: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_protocol_and_generic(&self, previous_node: &ast::Expr, previous_context: GenericContext<'db>, new_context: GenericContext<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_protocol_and_type_params(&self, source_node: &ast::Expr, type_params: &ast::TypeParams) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_variance(&self, class: StaticClassLiteral<'db>, base_alias: GenericAlias<'db>, source_node: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn nearest_disjoint_base(&self, base_class: ClassType<'db>) -> Result<Option<DisjointBase<'db>>, Self::Error>;
        #[operation(child)]
        async fn record_disjoint_base(&self, disjoint_bases: &mut IncompatibleBases<'db>, disjoint_base: DisjointBase<'db>, index: usize, base_class: ClassType<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_base_kind(&self, class: StaticClassLiteral<'db>, base_class: ClassType<'db>, source_node: &ast::Expr, is_protocol: bool, class_kind: Option<CodeGeneratorKind<'db>>, direct_typed_dict_bases: &mut Vec<ClassType<'db>>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn is_final(&self, base_class: ClassType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_final(&self, class: StaticClassLiteral<'db>, base_class: ClassType<'db>, source_node: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn static_class_literal(&self, base_class: ClassType<'db>) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;
        #[operation(child)]
        async fn is_frozen_dataclass(&self, class: StaticClassLiteral<'db>) -> Result<Option<bool>, Self::Error>;
        #[operation(child)]
        async fn report_frozen(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, base_class: StaticClassLiteral<'db>, source_node: &ast::Expr, base_is_frozen: bool) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn ordered_dataclass_base(&self, base_class: ClassType<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn has_own_comparison_methods(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_ordered(&self, class: StaticClassLiteral<'db>, ordered_base_class: ClassType<'db>, source_node: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_source_base<'node>(&self, class_node: &'node ast::StmtClassDef, cursor: &mut usize) -> Result<Option<&'node ast::Expr>, Self::Error>;
        #[operation(child)]
        async fn expression_type(&self, definition: Definition<'db>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn is_variable_length_tuple(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_unsupported(&self, class: StaticClassLiteral<'db>, source_node: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl ExplicitBaseCheckFacts {
        fn source_node<'node>(&self, entry: ExpandedClassBaseEntry<'node, '_>) -> &'node ast::Expr { entry.source_node() }
        fn base_type<'db>(&self, entry: ExpandedClassBaseEntry<'_, 'db>) -> Type<'db> { entry.ty() }
        fn type_params<'node>(&self, class_node: &'node ast::StmtClassDef) -> Option<&'node ast::TypeParams> { class_node.type_params.as_deref() }
        fn frozen_mismatch(&self, base_is_frozen: bool, class_is_frozen: bool) -> bool { base_is_frozen != class_is_frozen }
    }

    #[synchronous(check_explicit_bases_sync)]
    #[capabilities(effects = ExplicitBaseCheckEffects, facts = ExplicitBaseCheckFacts)]
    #[passive_values(ClassType::NonGeneric, ClassType::Generic, StaticClassBaseChecks)]
    pub(in crate::types::infer::builder) async fn check_explicit_bases_with<'node, 'db, E: ExplicitBaseCheckEffects<'db>>(
        class: StaticClassLiteral<'db>,
        class_node: &'node ast::StmtClassDef,
        class_kind: Option<CodeGeneratorKind<'db>>,
        is_protocol: bool,
        facts: ExplicitBaseCheckFacts,
        effects: &E,
    ) -> Result<StaticClassBaseChecks<'node, 'db>, E::Error> {
        let mut disjoint_bases = effects.empty_disjoint_bases().await?;
        #[passive_state]
        let mut protocol_base_with_generic_context: Option<(&ast::Expr, GenericContext<'db>)> = None;
        let mut direct_typed_dict_bases = effects.empty_typed_dict_bases().await?;
        let class_definition = effects.class_definition(class_node).await?;

        // Iterate through the class's explicit bases to check for various possible errors:
        //     - Check for inheritance from plain `Generic`,
        //     - Check for inheritance from a `@final` classes
        //     - If the class is a protocol class: check for inheritance from a non-protocol class
        //     - If the class is a NamedTuple class: check for multiple inheritance that isn't `Generic[]`
        let expanded_base_entries = effects.expand(class, class_node, class_definition).await?;
        let check_explicit_base_variance = effects.explicit_variance_enabled().await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(indexed_entry) = effects.next_entry(&expanded_base_entries, &mut cursor).await? {
            let (index, entry) = indexed_entry;
            let source_node = facts.source_node(entry);
            let base_class = facts.base_type(entry);
            effects.report_missing_arguments(base_class, source_node).await?;

            if matches!(class_kind, Some(CodeGeneratorKind::NamedTuple))
                && !matches!(base_class, Type::SpecialForm(SpecialFormType::NamedTuple) | Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_)))
            {
                effects.report_named_tuple(class, source_node).await?;
            }

            let base_class = match base_class {
                Type::SpecialForm(SpecialFormType::Generic) => {
                    effects.report_plain_generic(source_node).await?;
                    continue;
                }
                Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(new_context)) => {
                    let Some((previous_node, previous_context)) = protocol_base_with_generic_context else {
                        continue;
                    };
                    effects.report_protocol_and_generic(previous_node, previous_context, new_context).await?;
                    continue;
                }
                // Note that unlike several of the other errors caught in this function,
                // this does not lead to the class creation failing at runtime,
                // but it is semantically invalid.
                Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(generic_context)) => {
                    if let Some(type_params) = facts.type_params(class_node) {
                        effects.report_protocol_and_type_params(source_node, type_params).await?;
                    } else if matches!(protocol_base_with_generic_context, None) {
                        protocol_base_with_generic_context = Some((source_node, generic_context));
                    }
                    continue;
                }
                Type::ClassLiteral(base_class) => ClassType::NonGeneric(base_class),
                Type::GenericAlias(base_alias) => {
                    if check_explicit_base_variance {
                        effects.check_variance(class, base_alias, source_node).await?;
                    }
                    ClassType::Generic(base_alias)
                }
                _ => continue,
            };

            if let Some(disjoint_base) = effects.nearest_disjoint_base(base_class).await? {
                effects.record_disjoint_base(&mut disjoint_bases, disjoint_base, index, base_class).await?;
            }
            effects.check_base_kind(class, base_class, source_node, is_protocol, class_kind, &mut direct_typed_dict_bases).await?;

            if effects.is_final(base_class).await? {
                effects.report_final(class, base_class, source_node).await?;
            }

            if let Some(base_class_literal) = effects.static_class_literal(base_class).await?
                && let (Some(base_is_frozen), Some(class_is_frozen)) = (
                    effects.is_frozen_dataclass(base_class_literal).await?,
                    effects.is_frozen_dataclass(class).await?,
                )
                && facts.frozen_mismatch(base_is_frozen, class_is_frozen)
            {
                effects.report_frozen(class, class_node, base_class_literal, source_node, base_is_frozen).await?;
            }

            if let Some(ordered_base_class) = effects.ordered_dataclass_base(base_class).await? {
                // Suppress the diagnostic if the child class manually overrides all comparison
                // methods, since the user has explicitly fixed the LSP violation.
                if !effects.has_own_comparison_methods(class).await? {
                    effects.report_ordered(class, ordered_base_class, source_node).await?;
                }
            }
        }

        // Check for starred variable-length tuples that cannot be unpacked
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(base) = effects.next_source_base(class_node, &mut cursor).await? {
            if let ast::Expr::Starred(starred) = base {
                let starred_ty = effects.expression_type(class_definition, &starred.value).await?;
                if effects.is_variable_length_tuple(starred_ty).await? {
                    effects.report_unsupported(class, base, starred_ty).await?;
                }
            }
        }

        Ok(StaticClassBaseChecks {
            expanded_entries: expanded_base_entries,
            disjoint_bases,
            direct_typed_dict_bases,
        })
    }
}

/// Advances over retained entries without transferring their allocation into a fallible operation.
/// Controlled callers admit the cursor step before invoking this helper.
pub(in crate::types::infer::builder) fn next_expanded_base_entry<'node, 'db>(
    entries: &[ExpandedClassBaseEntry<'node, 'db>],
    cursor: &mut usize,
) -> Option<(usize, ExpandedClassBaseEntry<'node, 'db>)> {
    let entry = *entries.get(*cursor)?;
    let index = *cursor;
    *cursor += 1;
    Some((index, entry))
}

/// Advances within the caller's borrowed class node, which also owns the returned source expression.
/// Controlled callers admit the cursor step before invoking this helper.
pub(in crate::types::infer::builder) fn next_class_source_base<'node>(
    class_node: &'node ast::StmtClassDef,
    cursor: &mut usize,
) -> Option<&'node ast::Expr> {
    let base = class_node.bases().get(*cursor)?;
    *cursor += 1;
    Some(base)
}

impl<'db> SynchronousExplicitBaseCheckEffects<'db>
    for OrdinaryExplicitBaseCheckEffects<'_, 'db, '_>
{
    type Error = Infallible;

    fn empty_disjoint_bases(&self) -> Result<IncompatibleBases<'db>, Self::Error> {
        Ok(IncompatibleBases::default())
    }

    fn empty_typed_dict_bases(&self) -> Result<Vec<ClassType<'db>>, Self::Error> {
        Ok(Vec::new())
    }

    fn class_definition(
        &self,
        class_node: &ast::StmtClassDef,
    ) -> Result<Definition<'db>, Self::Error> {
        Ok(self.index.expect_single_definition(class_node))
    }

    fn expand<'node>(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &'node ast::StmtClassDef,
        definition: Definition<'db>,
    ) -> Result<Vec<ExpandedClassBaseEntry<'node, 'db>>, Self::Error> {
        let db = self.context.db();
        Ok(expanded_class_base_entries(
            db,
            class.known(db),
            class_node,
            definition,
        ))
    }

    fn explicit_variance_enabled(&self) -> Result<bool, Self::Error> {
        Ok(self.context.is_lint_enabled(&INVALID_GENERIC_CLASS))
    }

    fn next_entry<'node>(
        &self,
        entries: &[ExpandedClassBaseEntry<'node, 'db>],
        cursor: &mut usize,
    ) -> Result<Option<(usize, ExpandedClassBaseEntry<'node, 'db>)>, Self::Error> {
        Ok(next_expanded_base_entry(entries, cursor))
    }

    fn report_missing_arguments(
        &self,
        base: Type<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        report_missing_type_arguments(self.context, base, source_node);
        Ok(())
    }

    fn report_named_tuple(
        &self,
        class: StaticClassLiteral<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        if let Some(builder) = context.report_lint(&INVALID_NAMED_TUPLE, source_node) {
            builder.into_diagnostic(format_args!(
                "NamedTuple class `{}` cannot use multiple inheritance except with `Generic[]`",
                class.name(context.db()),
            ));
        }
        Ok(())
    }

    fn report_plain_generic(&self, source_node: &ast::Expr) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&INVALID_BASE, source_node) {
            // Unsubscripted `Generic` can appear in the MRO of many classes,
            // but it is never valid as an explicit base class in user code.
            builder.into_diagnostic("Cannot inherit from plain `Generic`");
        }
        Ok(())
    }

    fn report_protocol_and_generic(
        &self,
        previous_node: &ast::Expr,
        previous_context: GenericContext<'db>,
        new_context: GenericContext<'db>,
    ) -> Result<(), Self::Error> {
        let Some(builder) = self
            .context
            .report_lint(&INVALID_GENERIC_CLASS, previous_node)
        else {
            return Ok(());
        };
        let mut diagnostic = builder.into_diagnostic(
            "Cannot both inherit from subscripted `Protocol` \
                                and subscripted `Generic`",
        );
        if let ast::Expr::Subscript(previous_node) = previous_node
            && new_context == previous_context
        {
            diagnostic.help("Remove the type parameters from the `Protocol` base");
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(TextRange::new(
                previous_node.value.end(),
                previous_node.end(),
            ))));
        }
        Ok(())
    }

    fn report_protocol_and_type_params(
        &self,
        source_node: &ast::Expr,
        type_params: &ast::TypeParams,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let node = source_node;
        let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, node) else {
            return Ok(());
        };
        let mut diagnostic = builder.into_diagnostic(
            "Cannot both inherit from subscripted `Protocol` \
                            and use PEP 695 type variables",
        );
        if let ast::Expr::Subscript(node) = node {
            let source = source_text(context.db(), context.file());
            // The parser can recover a type parameter list without a closing bracket.
            if let Some(type_params) = source[type_params.range()].strip_prefix('[')
                && let Some(type_params) = type_params.strip_suffix(']')
                && type_params == &source[node.slice.range()]
            {
                diagnostic.help("Remove the type parameters from the `Protocol` base");
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(TextRange::new(
                    node.value.end(),
                    node.end(),
                ))));
            }
        }
        Ok(())
    }

    fn check_variance(
        &self,
        class: StaticClassLiteral<'db>,
        base_alias: GenericAlias<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        let env = context.program_environment();
        if let Some(generic_context) = class.generic_context(db)
            && let Some((typevar, declared_variance, required_variance)) =
                generic_context.variables(db).find_map(|typevar| {
                    let declared_variance = typevar.typevar(db).explicit_variance(db)?;
                    if declared_variance == TypeVarVariance::Invariant {
                        return None;
                    }
                    let required_variance = base_alias
                        .variance_of(db, env, typevar.identity(db))
                        .evaluate(db);
                    if declared_variance.join(required_variance) != declared_variance {
                        Some((typevar, declared_variance, required_variance))
                    } else {
                        None
                    }
                })
            && let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, source_node)
        {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Variance of type variable `{}` is incompatible with base class `{}`",
                typevar.typevar(db).name(db),
                base_alias.origin(db).name(db),
            ));
            diagnostic.help(format_args!(
                "Type variable `{}` is declared as {}, but base class `{}` requires it to be {}",
                typevar.typevar(db).name(db),
                declared_variance.as_str(),
                base_alias.origin(db).name(db),
                required_variance.as_str(),
            ));
        }
        Ok(())
    }

    fn nearest_disjoint_base(
        &self,
        base_class: ClassType<'db>,
    ) -> Result<Option<DisjointBase<'db>>, Self::Error> {
        Ok(base_class.nearest_disjoint_base(self.context.db()))
    }

    fn record_disjoint_base(
        &self,
        disjoint_bases: &mut IncompatibleBases<'db>,
        disjoint_base: DisjointBase<'db>,
        index: usize,
        base_class: ClassType<'db>,
    ) -> Result<(), Self::Error> {
        disjoint_bases.insert(
            disjoint_base,
            index,
            base_class.class_literal(self.context.db()),
        );
        Ok(())
    }

    fn check_base_kind(
        &self,
        class: StaticClassLiteral<'db>,
        base_class: ClassType<'db>,
        source_node: &ast::Expr,
        is_protocol: bool,
        class_kind: Option<CodeGeneratorKind<'db>>,
        direct_typed_dict_bases: &mut Vec<ClassType<'db>>,
    ) -> Result<(), Self::Error> {
        super::effects::check_explicit_base_kind(
            self.context,
            class,
            base_class,
            source_node,
            is_protocol,
            class_kind,
            direct_typed_dict_bases,
        );
        Ok(())
    }

    fn is_final(&self, base_class: ClassType<'db>) -> Result<bool, Self::Error> {
        Ok(base_class.is_final(self.context.db()))
    }

    fn report_final(
        &self,
        class: StaticClassLiteral<'db>,
        base_class: ClassType<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        if let Some(builder) = context.report_lint(&SUBCLASS_OF_FINAL_CLASS, source_node) {
            builder.into_diagnostic(format_args!(
                "Class `{}` cannot inherit from final class `{}`",
                class.name(db),
                base_class.name(db),
            ));
        }
        Ok(())
    }

    fn static_class_literal(
        &self,
        base_class: ClassType<'db>,
    ) -> Result<Option<StaticClassLiteral<'db>>, Self::Error> {
        Ok(base_class
            .static_class_literal(self.context.db())
            .map(|(class, _)| class))
    }

    fn is_frozen_dataclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<bool>, Self::Error> {
        Ok(class.is_frozen_dataclass(self.context.db()))
    }

    fn report_frozen(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        base_class: StaticClassLiteral<'db>,
        source_node: &ast::Expr,
        base_is_frozen: bool,
    ) -> Result<(), Self::Error> {
        report_bad_frozen_dataclass_inheritance(
            self.context,
            class,
            class_node,
            base_class,
            source_node,
            base_is_frozen,
        );
        Ok(())
    }

    fn ordered_dataclass_base(
        &self,
        base_class: ClassType<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error> {
        Ok(super::ordered_dataclass_base_class(
            self.context.db(),
            base_class,
        ))
    }

    fn has_own_comparison_methods(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.has_own_comparison_methods(self.context.db()))
    }

    fn report_ordered(
        &self,
        class: StaticClassLiteral<'db>,
        ordered_base_class: ClassType<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        if let Some(builder) = context.report_lint(&SUBCLASS_OF_DATACLASS_WITH_ORDER, source_node) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Class `{}` inherits from dataclass `{}` which has `order=True`",
                class.name(db),
                ordered_base_class.name(db),
            ));
            diagnostic.info(
                "Comparison of instances of the child class with instances \
                    of the parent class will raise `TypeError` at runtime",
            );
        }
        Ok(())
    }

    fn next_source_base<'node>(
        &self,
        class_node: &'node ast::StmtClassDef,
        cursor: &mut usize,
    ) -> Result<Option<&'node ast::Expr>, Self::Error> {
        Ok(next_class_source_base(class_node, cursor))
    }

    fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(definition_expression_type(
            self.context.db(),
            definition,
            expression,
        ))
    }

    fn is_variable_length_tuple(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(ty
            .tuple_instance_spec(self.context.db(), self.context.program_environment())
            .is_some_and(|spec| !matches!(spec.as_ref(), Tuple::Fixed(_))))
    }

    fn report_unsupported(
        &self,
        class: StaticClassLiteral<'db>,
        source_node: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        report_unsupported_base(self.context, source_node, ty, class);
        Ok(())
    }
}
