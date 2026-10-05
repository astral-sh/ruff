//! Validation of class declarations that require a value for `Final`.

use std::convert::Infallible;

use ty_python_core::definition::Definition;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{DeclarationsIterator, SemanticIndex};

use crate::TypeQualifiers;
use crate::place::{place_from_bindings, place_from_declarations};
use crate::types::StaticClassLiteral;
use crate::types::class::CodeGeneratorKind;
use crate::types::context::InferContext;
use crate::types::diagnostic::FINAL_WITHOUT_VALUE;
use crate::types::list_members::scope::ScopeMemberCursor;

pub(super) struct OrdinaryClassFinalValueEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
    pub(super) index: &'a SemanticIndex<'db>,
}

pub(in crate::types::infer::builder) struct ClassFinalValueFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassFinalValueEffects)]
    pub(in crate::types::infer::builder) trait ClassFinalValueEffects<'db> {
        type Error;

        #[operation(source)]
        async fn in_stub(&self) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn declaration_cursor<'index>(&'index self, class: StaticClassLiteral<'db>) -> Result<ScopeMemberCursor<'index, 'db>, Self::Error> where 'db: 'index;
        #[operation(child)]
        async fn class_kind(&self, class: StaticClassLiteral<'db>) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;
        #[operation(child)]
        async fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_declaration<'index>(&self, cursor: &mut ScopeMemberCursor<'index, 'db>) -> Result<Option<(ScopedSymbolId, DeclarationsIterator<'index, 'db>)>, Self::Error>;
        #[operation(child)]
        async fn declaration(&self, declarations: DeclarationsIterator<'_, 'db>) -> Result<(TypeQualifiers, Option<Definition<'db>>), Self::Error>;
        #[operation(child)]
        async fn check_missing_value(&self, class: StaticClassLiteral<'db>, symbol: ScopedSymbolId, first_declaration: Option<Definition<'db>>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl ClassFinalValueFacts {
        fn is_final(&self, qualifiers: TypeQualifiers) -> bool {
            qualifiers.contains(TypeQualifiers::FINAL)
        }
    }

    #[synchronous(check_class_final_without_value_sync)]
    #[capabilities(effects = ClassFinalValueEffects, facts = ClassFinalValueFacts)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_class_final_without_value_with<'db, E: ClassFinalValueEffects<'db>>(
        class: StaticClassLiteral<'db>,
        facts: ClassFinalValueFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        // In stub files, bare declarations without values are normal.
        if effects.in_stub().await? {
            return Ok(());
        }

        let mut cursor = effects.declaration_cursor(class).await?;

        // In dataclasses (and similar code-generated classes), Final fields without
        // defaults are initialized by the synthesized __init__. In protocols, the
        // declaration describes a required instance attribute rather than storage
        // that must be initialized by the protocol class itself.
        if matches!(effects.class_kind(class).await?, Some(_)) || effects.is_protocol(class).await? {
            return Ok(());
        }

        #[cursor_loop]
        while let Some(declaration) = effects.next_declaration(&mut cursor).await? {
            let (symbol, declarations) = declaration;
            let (qualifiers, first_declaration) = effects.declaration(declarations).await?;
            if !facts.is_final(qualifiers) {
                continue;
            }
            effects.check_missing_value(class, symbol, first_declaration).await?;
        }
        Ok(())
    }
}

impl<'db> SynchronousClassFinalValueEffects<'db> for OrdinaryClassFinalValueEffects<'_, 'db, '_> {
    type Error = Infallible;

    fn in_stub(&self) -> Result<bool, Self::Error> {
        Ok(self.context.in_stub())
    }

    fn declaration_cursor<'index>(
        &'index self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ScopeMemberCursor<'index, 'db>, Self::Error>
    where
        'db: 'index,
    {
        let db = self.context.db();
        let scope = class.body_scope(db);
        let scope_id = scope.file_scope_id(db);
        Ok(ScopeMemberCursor::new(
            scope,
            self.index.use_def_map(scope_id),
            self.index.place_table(scope_id),
        ))
    }

    fn class_kind(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        Ok(CodeGeneratorKind::from_class(
            self.context.db(),
            class.into(),
        ))
    }

    fn is_protocol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.is_protocol(self.context.db()))
    }

    fn next_declaration<'index>(
        &self,
        cursor: &mut ScopeMemberCursor<'index, 'db>,
    ) -> Result<Option<(ScopedSymbolId, DeclarationsIterator<'index, 'db>)>, Self::Error> {
        Ok(cursor.next_declaration())
    }

    fn declaration(
        &self,
        declarations: DeclarationsIterator<'_, 'db>,
    ) -> Result<(TypeQualifiers, Option<Definition<'db>>), Self::Error> {
        let result = place_from_declarations(
            self.context.db(),
            self.context.program_environment(),
            declarations,
        );
        let first_declaration = result.first_declaration;
        let (place_and_quals, _) = result.into_place_and_conflicting_declarations();
        Ok((place_and_quals.qualifiers, first_declaration))
    }

    fn check_missing_value(
        &self,
        class: StaticClassLiteral<'db>,
        symbol_id: ScopedSymbolId,
        first_declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        let body_scope = class.body_scope(db);
        let body_scope_id = body_scope.file_scope_id(db);
        let use_def = self.index.use_def_map(body_scope_id);
        let place_table = self.index.place_table(body_scope_id);

        // Check if the symbol has any bindings at class level.
        let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
        let binding_place = place_from_bindings(db, context.program_environment(), bindings);

        if !binding_place.place.is_undefined() {
            return Ok(());
        }

        // Per the typing spec, a `Final` attribute declared in a class body without a
        // value must be initialized in `__init__`. Assignments in other methods don't count.
        let symbol = place_table.symbol(symbol_id);
        if super::has_binding_in_init(context, body_scope, self.index, symbol.name().as_str()) {
            return Ok(());
        }

        let place = place_table.place(symbol_id);
        if let Some(first_decl) = first_declaration
            && let Some(builder) = context.report_lint(
                &FINAL_WITHOUT_VALUE,
                first_decl.full_range(db, context.module()),
            )
        {
            builder.into_diagnostic(format_args!(
                "`Final` symbol `{place}` is not assigned a value"
            ));
        }
        Ok(())
    }
}
