//! Validation of the metaclass selected for a static class.

use std::convert::Infallible;

use itertools::Either;
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};

use crate::types::class::MetaclassErrorKind;
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    CONFLICTING_METACLASS, CYCLIC_CLASS_DEFINITION, INVALID_METACLASS,
    report_conflicting_metaclass_from_bases,
};
use crate::types::{
    ClassBase, ClassLiteral, ClassType, DisplaySettings, MetaclassCandidate, StaticClassLiteral,
    Type,
};

pub(super) struct OrdinaryMetaclassCheckEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousMetaclassCheckEffects)]
    pub(in crate::types::infer::builder) trait MetaclassCheckEffects<'db> {
        type Error;

        #[operation(child)]
        async fn metaclass_error(&self, class: StaticClassLiteral<'db>) -> Result<Option<MetaclassErrorKind<'db>>, Self::Error>;
        #[operation(child)]
        async fn invalid_metaclass_range(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef) -> Result<TextRange, Self::Error>;
        #[operation(child)]
        async fn report_cycle(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_generic(&self, range: TextRange) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_not_callable(&self, range: TextRange, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_partly_not_callable(&self, range: TextRange, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_conflict(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef, candidate: MetaclassCandidate<'db>, base_metaclass: ClassType<'db>, base: ClassBase<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(check_metaclass_sync)]
    #[capabilities(effects = MetaclassCheckEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_metaclass_with<'db, E: MetaclassCheckEffects<'db>>(
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
        effects: &E,
    ) -> Result<(), E::Error> {
        let Some(error) = effects.metaclass_error(class).await? else {
            return Ok(());
        };
        let range = effects.invalid_metaclass_range(class, node).await?;
        match error {
            MetaclassErrorKind::Cycle => effects.report_cycle(class, node).await,
            MetaclassErrorKind::GenericMetaclass => effects.report_generic(range).await,
            MetaclassErrorKind::NotCallable(ty) => effects.report_not_callable(range, ty).await,
            MetaclassErrorKind::PartlyNotCallable(ty) => effects.report_partly_not_callable(range, ty).await,
            MetaclassErrorKind::Conflict { candidate, base_metaclass, base, .. } => {
                effects.report_conflict(class, node, candidate, base_metaclass, base).await
            }
        }
    }
}

impl<'db> SynchronousMetaclassCheckEffects<'db> for OrdinaryMetaclassCheckEffects<'_, 'db, '_> {
    type Error = Infallible;

    fn metaclass_error(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<MetaclassErrorKind<'db>>, Self::Error> {
        Ok(class
            .try_metaclass(self.context.db())
            .err()
            .map(|error| error.reason().clone()))
    }

    fn invalid_metaclass_range(
        &self,
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
    ) -> Result<TextRange, Self::Error> {
        Ok(node
            .arguments
            .as_ref()
            .and_then(|arguments| arguments.find_keyword("metaclass"))
            .map(Ranged::range)
            .unwrap_or_else(|| class.header_range(self.context.db())))
    }

    fn report_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
    ) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&CYCLIC_CLASS_DEFINITION, node) {
            builder.into_diagnostic(format_args!(
                "Cyclic definition of `{}`",
                class.name(self.context.db())
            ));
        }
        Ok(())
    }

    fn report_generic(&self, range: TextRange) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&INVALID_METACLASS, range) {
            builder.into_diagnostic("Generic metaclasses are not supported");
        }
        Ok(())
    }

    fn report_not_callable(&self, range: TextRange, ty: Type<'db>) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&INVALID_METACLASS, range) {
            builder.into_diagnostic(format_args!(
                "Metaclass type `{}` is not callable",
                ty.display(self.context.db(), self.context.program_environment())
            ));
        }
        Ok(())
    }

    fn report_partly_not_callable(
        &self,
        range: TextRange,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&INVALID_METACLASS, range) {
            builder.into_diagnostic(format_args!(
                "Metaclass type `{}` is partly not callable",
                ty.display(self.context.db(), self.context.program_environment())
            ));
        }
        Ok(())
    }

    fn report_conflict(
        &self,
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
        candidate: MetaclassCandidate<'db>,
        base_metaclass: ClassType<'db>,
        base: ClassBase<'db>,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        let env = context.program_environment();
        let MetaclassCandidate {
            metaclass: metaclass1,
            base: base1,
        } = candidate;
        let metaclass2 = base_metaclass;
        let base2 = base;
        if let Some(base1) = base1 {
            report_conflicting_metaclass_from_bases(
                context,
                node.into(),
                class.name(db),
                metaclass1,
                base1.name(db),
                metaclass2,
                base2.name(db),
            );
        } else if let Some(builder) = context.report_lint(&CONFLICTING_METACLASS, node) {
            let types = [
                Type::from(class),
                Type::from(metaclass1),
                Type::from(metaclass2),
                Type::from(base2),
            ];
            let settings = DisplaySettings::from_possibly_ambiguous_types(db, env, types);
            let base = if let ClassBase::Class(base) = base2 {
                Either::Left(base.class_literal(db).display_with(db, settings.clone()))
            } else {
                Either::Right(base2.display_with(db, env, settings.clone()))
            };
            builder.into_diagnostic(format_args!(
                "The metaclass of a derived class (`{class}`) \
                    must be a subclass of the metaclasses of all its bases, \
                    but `{metaclass_of_class}` (metaclass of `{class}`) \
                    and `{metaclass_of_base}` (metaclass of base class `{base}`) \
                    have no subclass relationship",
                class = ClassLiteral::Static(class).display_with(db, settings.clone()),
                metaclass_of_class = metaclass1
                    .class_literal(db)
                    .display_with(db, settings.clone()),
                metaclass_of_base = metaclass2.class_literal(db).display_with(db, settings),
            ));
        }
        Ok(())
    }
}
