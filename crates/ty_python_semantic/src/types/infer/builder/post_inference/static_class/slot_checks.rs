//! Shared decisions for explicit slot layouts and class-namespace conflicts.

use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::SemanticIndex;

use crate::types::context::InferContext;
use crate::types::diagnostic::{INVALID_ASSIGNMENT, INVALID_DATACLASS};
use crate::types::{DataclassFlags, StaticClassLiteral};

pub(super) struct OrdinaryClassSlotCheckEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
    pub(super) index: &'a SemanticIndex<'db>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassSlotCheckEffects)]
    pub(in crate::types::infer::builder) trait ClassSlotCheckEffects<'db> {
        type Error;

        #[operation(child)]
        async fn has_explicit_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn dataclass_has_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_dataclass_conflict(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn in_stub(&self) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn slot_names(&self, class: StaticClassLiteral<'db>) -> Result<Option<&'db [Name]>, Self::Error>;
        #[operation(child)]
        async fn check_namespace(&self, class: StaticClassLiteral<'db>, slot_names: &[Name]) -> Result<(), Self::Error>;
    }

    #[synchronous(check_class_slots_sync)]
    #[capabilities(effects = ClassSlotCheckEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_class_slots_with<'db, E: ClassSlotCheckEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let has_explicit_slots = effects.has_explicit_slots(class).await?;

        if has_explicit_slots && effects.dataclass_has_slots(class).await? {
            effects.report_dataclass_conflict(class).await?;
            return Ok(());
        }

        if !has_explicit_slots || effects.in_stub().await? {
            return Ok(());
        }

        let Some(slot_names) = effects.slot_names(class).await? else {
            return Ok(());
        };

        effects.check_namespace(class, slot_names).await
    }
}

impl<'db> SynchronousClassSlotCheckEffects<'db> for OrdinaryClassSlotCheckEffects<'_, 'db, '_> {
    type Error = Infallible;

    fn has_explicit_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.has_explicit_slots(self.context.db()))
    }

    fn dataclass_has_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        let db = self.context.db();
        Ok(class
            .dataclass_params(db)
            .is_some_and(|parameters| parameters.flags(db).contains(DataclassFlags::SLOTS)))
    }

    fn report_dataclass_conflict(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        if let Some(builder) = context.report_lint(&INVALID_DATACLASS, class.header_range(db)) {
            builder.into_diagnostic(format_args!(
                "Dataclass `{}` cannot combine `slots=True` with manually assigned `__slots__`",
                class.name(db),
            ));
        }
        Ok(())
    }

    fn in_stub(&self) -> Result<bool, Self::Error> {
        Ok(self.context.in_stub())
    }

    fn slot_names(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<&'db [Name]>, Self::Error> {
        Ok(class.slot_names(self.context.db()))
    }

    fn check_namespace(
        &self,
        class: StaticClassLiteral<'db>,
        slot_names: &[Name],
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        let index = self.index;
        let scope_id = class.body_scope(db).file_scope_id(db);
        let table = index.place_table(scope_id);
        let use_def = index.use_def_map(scope_id);

        for name in slot_names {
            let Some(symbol) = table.symbol_id(name) else {
                continue;
            };

            for binding in use_def.end_of_scope_symbol_bindings(symbol) {
                if let Some(definition) = binding.binding.definition()
                    && !index.is_in_type_checking_block(
                        scope_id,
                        definition.kind(db).full_range(context.module()),
                    )
                    && let Some(builder) = context.report_lint(
                        &INVALID_ASSIGNMENT,
                        definition.focus_range(db, context.module()),
                    )
                {
                    builder.into_diagnostic(format_args!(
                        "Class variable `{name}` conflicts with an instance slot"
                    ));
                }
            }
        }
        Ok(())
    }
}
