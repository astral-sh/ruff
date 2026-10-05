//! Shared field-order and explicit-base decisions for static classes.

use std::convert::Infallible;

use ruff_db::diagnostic::Annotation;
use ruff_python_ast::{self as ast, name::Name};
use ty_python_core::definition::Definition;

use crate::FxIndexMap;
use crate::types::class::{CodeGeneratorKind, Field, FieldKind};
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    INVALID_PROTOCOL, INVALID_TYPED_DICT_HEADER, report_named_tuple_field_with_leading_underscore,
    report_namedtuple_field_without_default_after_field_with_default,
};
use crate::types::{ClassType, StaticClassLiteral};

#[cfg(test)]
mod tests;

type PreviousNamedTupleDefault<'db> = Option<(Name, Option<Definition<'db>>)>;

#[derive(Clone, Copy)]
struct StaticClassFacts;

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousStaticClassLocalEffects)]
trait StaticClassLocalEffects {
    type Error;

    #[operation(local)]
    #[progress]
    async fn next_map_entry<'a, K, V>(
        &self,
        fields: &'a FxIndexMap<K, V>,
        cursor: &mut usize,
    ) -> Result<Option<(&'a K, &'a V)>, Self::Error>;

    #[operation(local)]
    async fn remember_named_tuple_default<'db>(
        &self,
        previous: &mut PreviousNamedTupleDefault<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error>;

    #[operation(local)]
    async fn append_copy<T: Copy>(&self, values: &mut Vec<T>, value: T) -> Result<(), Self::Error>;
}

#[synchronous(SynchronousNamedTupleFieldEffects)]
trait NamedTupleFieldEffects<'db>: StaticClassLocalEffects {
    #[operation(child)]
    async fn named_tuple_fields(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db FxIndexMap<Name, Field<'db>>, Self::Error>;

    #[operation(child)]
    async fn report_leading_underscore(
        &self,
        class: StaticClassLiteral<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error>;

    #[operation(child)]
    async fn report_required_after_default(
        &self,
        class: StaticClassLiteral<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
        previous: &(Name, Option<Definition<'db>>),
    ) -> Result<(), Self::Error>;
}

#[synchronous(SynchronousExplicitBaseKindEffects)]
trait ExplicitBaseKindEffects<'db>: StaticClassLocalEffects {
    #[operation(child)]
    async fn base_is_protocol(&self, base: ClassType<'db>) -> Result<bool, Self::Error>;

    #[operation(child)]
    async fn base_is_object(&self, base: ClassType<'db>) -> Result<bool, Self::Error>;

    #[operation(child)]
    async fn base_is_typed_dict(&self, base: ClassType<'db>) -> Result<bool, Self::Error>;

    #[operation(child)]
    async fn report_invalid_protocol_base(
        &self,
        class: StaticClassLiteral<'db>,
        base: ClassType<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error>;

    #[operation(child)]
    async fn report_invalid_typed_dict_base(
        &self,
        class: StaticClassLiteral<'db>,
        base: ClassType<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Self::Error>;
}

#[finite_capability]
impl StaticClassFacts {
    fn starts_with_underscore(&self, name: &Name) -> bool {
        name.starts_with('_')
    }

    fn previous_default<'a, 'db>(
        &self,
        previous: &'a PreviousNamedTupleDefault<'db>,
    ) -> Option<&'a (Name, Option<Definition<'db>>)> {
        previous.as_ref()
    }
}

#[synchronous(check_named_tuple_fields_sync)]
#[capabilities(effects = NamedTupleFieldEffects, facts = StaticClassFacts)]
#[passive_values()]
async fn check_named_tuple_fields_with<'db, E: NamedTupleFieldEffects<'db>>(
    class: StaticClassLiteral<'db>,
    facts: StaticClassFacts,
    effects: &E,
) -> Result<(), E::Error> {
    let fields = effects.named_tuple_fields(class).await?;
    let mut cursor = 0;
    let mut previous = None;
    #[cursor_loop]
    while let Some(entry) = effects.next_map_entry(fields, &mut cursor).await? {
        let (name, field) = entry;
        if facts.starts_with_underscore(name) {
            effects.report_leading_underscore(class, name, field.first_declaration).await?;
        }
        if matches!(field.kind, FieldKind::NamedTuple { default_ty: Some(_) }) {
            effects.remember_named_tuple_default(&mut previous, name, field.first_declaration).await?;
        } else if let Some(default) = facts.previous_default(&previous) {
            effects.report_required_after_default(class, name, field.first_declaration, default).await?;
        }
    }
    Ok(())
}

#[synchronous(check_explicit_base_kind_sync)]
#[capabilities(effects = ExplicitBaseKindEffects)]
#[passive_values()]
async fn check_explicit_base_kind_with<'db, E: ExplicitBaseKindEffects<'db>>(
    class: StaticClassLiteral<'db>,
    base: ClassType<'db>,
    source_node: &ast::Expr,
    is_protocol: bool,
    class_kind: Option<CodeGeneratorKind<'db>>,
    direct_typed_dict_bases: &mut Vec<ClassType<'db>>,
    effects: &E,
) -> Result<(), E::Error> {
    if is_protocol {
        if !effects.base_is_protocol(base).await? && !effects.base_is_object(base).await? {
            effects.report_invalid_protocol_base(class, base, source_node).await?;
        }
    } else if matches!(class_kind, Some(CodeGeneratorKind::TypedDict)) {
        if !effects.base_is_typed_dict(base).await? {
            effects.report_invalid_typed_dict_base(class, base, source_node).await?;
        }
        if effects.base_is_typed_dict(base).await? {
            effects.append_copy(direct_typed_dict_bases, base).await?;
        }
    }
    Ok(())
}
}

// These commits invoke no semantic operation or caller-supplied callback. Controlled callers
// perform their final completion check before entering a commit and retain the owners outside it.
fn next_map_entry<'a, K, V>(
    fields: &'a FxIndexMap<K, V>,
    cursor: &mut usize,
) -> Option<(&'a K, &'a V)> {
    let entry = fields.get_index(*cursor)?;
    *cursor += 1;
    Some(entry)
}

fn remember_named_tuple_default<'db>(
    previous: &mut PreviousNamedTupleDefault<'db>,
    name: &Name,
    declaration: Option<Definition<'db>>,
) {
    *previous = Some((name.clone(), declaration));
}

fn append_copy<T: Copy>(values: &mut Vec<T>, value: T) {
    values.push(value);
}

struct OrdinaryStaticClassEffects<'a, 'db, 'ast> {
    context: &'a InferContext<'db, 'ast>,
}

impl SynchronousStaticClassLocalEffects for OrdinaryStaticClassEffects<'_, '_, '_> {
    type Error = Infallible;

    fn next_map_entry<'a, K, V>(
        &self,
        fields: &'a FxIndexMap<K, V>,
        cursor: &mut usize,
    ) -> Result<Option<(&'a K, &'a V)>, Infallible> {
        Ok(next_map_entry(fields, cursor))
    }

    fn remember_named_tuple_default<'db>(
        &self,
        previous: &mut PreviousNamedTupleDefault<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Infallible> {
        remember_named_tuple_default(previous, name, declaration);
        Ok(())
    }

    fn append_copy<T: Copy>(&self, values: &mut Vec<T>, value: T) -> Result<(), Infallible> {
        append_copy(values, value);
        Ok(())
    }
}

impl<'db> SynchronousNamedTupleFieldEffects<'db> for OrdinaryStaticClassEffects<'_, 'db, '_> {
    fn named_tuple_fields(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db FxIndexMap<Name, Field<'db>>, Infallible> {
        Ok(class.own_fields(self.context.db(), None, CodeGeneratorKind::NamedTuple))
    }

    fn report_leading_underscore(
        &self,
        class: StaticClassLiteral<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
    ) -> Result<(), Infallible> {
        report_named_tuple_field_with_leading_underscore(self.context, class, name, declaration);
        Ok(())
    }

    fn report_required_after_default(
        &self,
        class: StaticClassLiteral<'db>,
        name: &Name,
        declaration: Option<Definition<'db>>,
        previous: &(Name, Option<Definition<'db>>),
    ) -> Result<(), Infallible> {
        report_namedtuple_field_without_default_after_field_with_default(
            self.context,
            class,
            (name, declaration),
            previous,
        );
        Ok(())
    }
}

impl<'db> SynchronousExplicitBaseKindEffects<'db> for OrdinaryStaticClassEffects<'_, 'db, '_> {
    fn base_is_protocol(&self, base: ClassType<'db>) -> Result<bool, Infallible> {
        Ok(base.is_protocol(self.context.db()))
    }

    fn base_is_object(&self, base: ClassType<'db>) -> Result<bool, Infallible> {
        Ok(base.is_object(self.context.db()))
    }

    fn base_is_typed_dict(&self, base: ClassType<'db>) -> Result<bool, Infallible> {
        let db = self.context.db();
        Ok(base.class_literal(db).is_typed_dict(db))
    }

    fn report_invalid_protocol_base(
        &self,
        class: StaticClassLiteral<'db>,
        base_class: ClassType<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Infallible> {
        let context = self.context;
        let db = context.db();
        if let Some(builder) = context.report_lint(&INVALID_PROTOCOL, source_node) {
            builder.into_diagnostic(format_args!(
                "Protocol class `{}` cannot inherit from non-protocol class `{}`",
                class.name(db),
                base_class.name(db),
            ));
        }
        Ok(())
    }

    fn report_invalid_typed_dict_base(
        &self,
        class: StaticClassLiteral<'db>,
        base_class: ClassType<'db>,
        source_node: &ast::Expr,
    ) -> Result<(), Infallible> {
        let context = self.context;
        let db = context.db();
        if let Some(builder) = context.report_lint(&INVALID_TYPED_DICT_HEADER, source_node) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "TypedDict class `{}` can only inherit from TypedDict classes",
                class.name(db),
            ));
            diagnostic.set_primary_annotation_message(format_args!(
                "`{}` is not a `TypedDict` class",
                base_class.name(db)
            ));
            diagnostic.annotate(
                Annotation::secondary(base_class.class_literal(db).header_span(db))
                    .message(format_args!("`{}` defined here", base_class.name(db))),
            );
        }
        Ok(())
    }
}

pub(super) fn check_named_tuple_fields<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
) {
    let result = check_named_tuple_fields_sync(
        class,
        StaticClassFacts,
        &OrdinaryStaticClassEffects { context },
    );
    match result {
        Ok(()) => (),
        Err(never) => match never {},
    }
}

pub(super) fn check_explicit_base_kind<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    base: ClassType<'db>,
    source_node: &ast::Expr,
    is_protocol: bool,
    class_kind: Option<CodeGeneratorKind<'db>>,
    direct_typed_dict_bases: &mut Vec<ClassType<'db>>,
) {
    let result = check_explicit_base_kind_sync(
        class,
        base,
        source_node,
        is_protocol,
        class_kind,
        direct_typed_dict_bases,
        &OrdinaryStaticClassEffects { context },
    );
    match result {
        Ok(()) => (),
        Err(never) => match never {},
    }
}
