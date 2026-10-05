//! Validation of canonical MRO results, layout constraints, and generic base arguments.

use std::convert::Infallible;

use itertools::Itertools;
use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};

use super::phases::StaticClassBaseChecks;
use crate::types::class::ExpandedClassBaseEntry;
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    CYCLIC_CLASS_DEFINITION, INCONSISTENT_MRO, INVALID_GENERIC_CLASS, IncompatibleBases,
    report_duplicate_bases, report_inconsistent_generic_bases, report_instance_layout_conflict,
    report_invalid_or_unsupported_base,
};
use crate::types::mro::{DuplicateBaseError, StaticMroErrorKind};
use crate::types::{StaticClassLiteral, Type};

pub(super) struct OrdinaryMroCheckEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousMroCheckEffects)]
    pub(in crate::types::infer::builder) trait MroCheckEffects<'db> {
        type Error;

        #[operation(child)]
        async fn mro_error(&self, class: StaticClassLiteral<'db>) -> Result<Option<&'db StaticMroErrorKind<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_duplicate<'a>(&self, duplicates: &'a [DuplicateBaseError<'db>], cursor: &mut usize) -> Result<Option<&'a DuplicateBaseError<'db>>, Self::Error>;
        #[operation(child)]
        async fn report_duplicate(&self, class: StaticClassLiteral<'db>, duplicate: &DuplicateBaseError<'db>, entries: &[ExpandedClassBaseEntry<'_, 'db>]) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_invalid(&self, invalid: &[(usize, Type<'db>)], cursor: &mut usize) -> Result<Option<(usize, Type<'db>)>, Self::Error>;
        #[operation(child)]
        async fn report_invalid(&self, class: StaticClassLiteral<'db>, index: usize, ty: Type<'db>, entries: &[ExpandedClassBaseEntry<'_, 'db>]) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_unresolvable(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef, bases: &[Type<'db>], generic_index: &Option<usize>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_pep695(&self, node: &ast::StmtClassDef) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_cycle(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn prune_disjoint(&self, bases: &mut IncompatibleBases<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn has_layout_conflict(&self, bases: &IncompatibleBases<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_layout(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef, bases: &IncompatibleBases<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        async fn source_nodes<'node>(&self, node: &'node ast::StmtClassDef, bases: &[Type<'db>]) -> Result<Option<&'node [ast::Expr]>, Self::Error>;
        #[operation(child)]
        async fn check_generic(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef, bases: &[Type<'db>], source_nodes: Option<&[ast::Expr]>) -> Result<bool, Self::Error>;
    }

    #[synchronous(check_mro_sync)]
    #[capabilities(effects = MroCheckEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_mro_with<'db, E: MroCheckEffects<'db>>(
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
        bases: &mut StaticClassBaseChecks<'_, 'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        if let Some(error) = effects.mro_error(class).await? {
            match error {
                StaticMroErrorKind::DuplicateBases(duplicates) => {
                    let mut cursor = 0;
                    #[cursor_loop]
                    while let Some(duplicate) = effects.next_duplicate(duplicates, &mut cursor).await? {
                        effects.report_duplicate(class, duplicate, &bases.expanded_entries).await?;
                    }
                }
                StaticMroErrorKind::InvalidBases(invalid) => {
                    let mut cursor = 0;
                    #[cursor_loop]
                    while let Some(entry) = effects.next_invalid(invalid, &mut cursor).await? {
                        let (index, ty) = entry;
                        effects.report_invalid(class, index, ty, &bases.expanded_entries).await?;
                    }
                }
                StaticMroErrorKind::UnresolvableMro { bases_list, generic_index } => {
                    effects.report_unresolvable(class, node, bases_list, generic_index).await?;
                }
                StaticMroErrorKind::Pep695ClassWithGenericInheritance => {
                    effects.report_pep695(node).await?;
                }
                StaticMroErrorKind::InheritanceCycle => {
                    effects.report_cycle(class, node).await?;
                }
            }
            return Ok(false);
        }

        effects.prune_disjoint(&mut bases.disjoint_bases).await?;
        if effects.has_layout_conflict(&bases.disjoint_bases).await? {
            effects.report_layout(class, node, &bases.disjoint_bases).await?;
        }
        let explicit_bases = effects.explicit_bases(class).await?;
        let source_nodes = effects.source_nodes(node, explicit_bases).await?;
        effects.check_generic(class, node, explicit_bases, source_nodes).await
    }
}

pub(in crate::types::infer::builder) fn next_duplicate<'a, 'db>(
    duplicates: &'a [DuplicateBaseError<'db>],
    cursor: &mut usize,
) -> Option<&'a DuplicateBaseError<'db>> {
    let duplicate = duplicates.get(*cursor)?;
    *cursor += 1;
    Some(duplicate)
}

pub(in crate::types::infer::builder) fn next_invalid<'db>(
    invalid: &[(usize, Type<'db>)],
    cursor: &mut usize,
) -> Option<(usize, Type<'db>)> {
    let entry = *invalid.get(*cursor)?;
    *cursor += 1;
    Some(entry)
}

/// An expanded base has a unique source node only when no source base was unpacked.
/// Controlled callers admit the source-base scan before invoking this helper.
pub(in crate::types::infer::builder) fn base_source_nodes(
    node: &ast::StmtClassDef,
    base_count: usize,
) -> Option<&[ast::Expr]> {
    if node.bases().len() == base_count && !node.bases().iter().any(ast::Expr::is_starred_expr) {
        Some(node.bases())
    } else {
        None
    }
}

impl<'db> SynchronousMroCheckEffects<'db> for OrdinaryMroCheckEffects<'_, 'db, '_> {
    type Error = Infallible;

    fn mro_error(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<&'db StaticMroErrorKind<'db>>, Self::Error> {
        Ok(class
            .try_mro(self.context.db(), None)
            .err()
            .map(|error| error.reason()))
    }

    fn next_duplicate<'a>(
        &self,
        duplicates: &'a [DuplicateBaseError<'db>],
        cursor: &mut usize,
    ) -> Result<Option<&'a DuplicateBaseError<'db>>, Self::Error> {
        Ok(next_duplicate(duplicates, cursor))
    }

    fn report_duplicate(
        &self,
        class: StaticClassLiteral<'db>,
        duplicate: &DuplicateBaseError<'db>,
        entries: &[ExpandedClassBaseEntry<'_, 'db>],
    ) -> Result<(), Self::Error> {
        report_duplicate_bases(self.context, class, duplicate, entries);
        Ok(())
    }

    fn next_invalid(
        &self,
        invalid: &[(usize, Type<'db>)],
        cursor: &mut usize,
    ) -> Result<Option<(usize, Type<'db>)>, Self::Error> {
        Ok(next_invalid(invalid, cursor))
    }

    fn report_invalid(
        &self,
        class: StaticClassLiteral<'db>,
        index: usize,
        ty: Type<'db>,
        entries: &[ExpandedClassBaseEntry<'_, 'db>],
    ) -> Result<(), Self::Error> {
        report_invalid_or_unsupported_base(self.context, entries[index].source_node(), ty, class);
        Ok(())
    }

    fn report_unresolvable(
        &self,
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
        bases: &[Type<'db>],
        generic_index: &Option<usize>,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        let env = context.program_environment();
        if let Some(builder) = context.report_lint(&INCONSISTENT_MRO, class.header_range(db)) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Cannot create a consistent method resolution order (MRO) \
                 for class `{}` with bases list `[{}]`",
                class.name(db),
                bases.iter().map(|base| base.display(db, env)).join(", ")
            ));
            let can_rewrite_bases = bases.len() == node.bases().len()
                && !node.bases().iter().any(ast::Expr::is_starred_expr);
            if can_rewrite_bases
                && let Some(index) = *generic_index
                && let [first_base, .., last_base] = node.bases()
            {
                let source = source_text(db, context.file());
                let generic_base = &source[node.bases()[index].range()];
                diagnostic.help(format_args!(
                    "Move `{generic_base}` to the end of the bases list"
                ));
                let reordered_bases = node
                    .bases()
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != index)
                    .map(|(_, base)| &source[base.range()])
                    .chain(std::iter::once(generic_base))
                    .join(", ");
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    reordered_bases,
                    TextRange::new(first_base.start(), last_base.end()),
                )));
            }
        }
        Ok(())
    }

    fn report_pep695(&self, node: &ast::StmtClassDef) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&INVALID_GENERIC_CLASS, node) {
            builder.into_diagnostic(
                "Cannot both inherit from `typing.Generic` \
                and use PEP 695 type variables",
            );
        }
        Ok(())
    }

    fn report_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
    ) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&CYCLIC_CLASS_DEFINITION, node) {
            builder.into_diagnostic(format_args!(
                "Cyclic definition of `{}` (class cannot inherit from itself)",
                class.name(self.context.db())
            ));
        }
        Ok(())
    }

    fn prune_disjoint(&self, bases: &mut IncompatibleBases<'db>) -> Result<(), Self::Error> {
        bases.remove_redundant_entries(self.context.db());
        Ok(())
    }

    fn has_layout_conflict(&self, bases: &IncompatibleBases<'db>) -> Result<bool, Self::Error> {
        Ok(bases.len() > 1)
    }

    fn report_layout(
        &self,
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
        bases: &IncompatibleBases<'db>,
    ) -> Result<(), Self::Error> {
        report_instance_layout_conflict(
            self.context,
            class.header_range(self.context.db()),
            Some(node.bases()),
            bases,
        );
        Ok(())
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(class.explicit_bases(self.context.db()))
    }

    fn source_nodes<'node>(
        &self,
        node: &'node ast::StmtClassDef,
        bases: &[Type<'db>],
    ) -> Result<Option<&'node [ast::Expr]>, Self::Error> {
        Ok(base_source_nodes(node, bases.len()))
    }

    fn check_generic(
        &self,
        class: StaticClassLiteral<'db>,
        _node: &ast::StmtClassDef,
        bases: &[Type<'db>],
        source_nodes: Option<&[ast::Expr]>,
    ) -> Result<bool, Self::Error> {
        Ok(report_inconsistent_generic_bases(
            self.context,
            class.header_range(self.context.db()),
            bases,
            source_nodes,
        ))
    }
}
