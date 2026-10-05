//! Check each inheritance path for incompatible concrete generic arguments.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;

use crate::types::class::ExplicitClassAncestors;
use crate::types::context::InferContext;
use crate::types::{ClassLiteral, ClassType, GenericAlias, StaticClassLiteral, Type};

use super::INVALID_GENERIC_CLASS;

#[cfg(test)]
mod tests;

/// A type parameter of a generic ancestor, independent of its specialization.
#[derive(PartialEq, Eq, Hash, Debug)]
pub(in crate::types) struct GenericBaseParameter<'db> {
    origin: StaticClassLiteral<'db>,
    parameter_index: usize,
}

/// A non-dynamic type argument and the inheritance path that supplies it.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct GenericBaseConstraint<'db> {
    argument: Type<'db>,
    alias: GenericAlias<'db>,
    /// The index in the class's explicit bases list, used to locate the diagnostic annotation.
    base_index: usize,
}

impl<'db> GenericBaseConstraint<'db> {
    pub(in crate::types) fn has_argument(self, argument: Type<'db>) -> bool {
        self.argument == argument
    }

    pub(in crate::types) fn has_base(self, base_index: usize) -> bool {
        self.base_index == base_index
    }
}

pub(in crate::types) type GenericBaseConstraints<'db> =
    FxHashMap<GenericBaseParameter<'db>, GenericBaseConstraint<'db>>;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousGenericBaseCheckEffects)]
    pub(in crate::types) trait GenericBaseCheckEffects<'db> {
        type Error;
        type Ancestors<'state> where Self: 'state;

        #[operation(local)]
        async fn empty_constraints(&self) -> Result<GenericBaseConstraints<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type(&self, types: &[Type<'db>], cursor: &mut usize) -> Result<Option<(usize, Type<'db>)>, Self::Error>;
        #[operation(child)]
        async fn has_generic_context(&self, class: ClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn ancestors_start(&self, class: ClassType<'db>) -> Result<Self::Ancestors<'_>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn ancestors_next<'state>(&'state self, cursor: &mut Self::Ancestors<'state>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(source)]
        async fn origin(&self, alias: GenericAlias<'db>) -> Result<StaticClassLiteral<'db>, Self::Error>;
        #[operation(source)]
        async fn arguments(&self, alias: GenericAlias<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        async fn is_dynamic(&self, argument: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn remember_argument(&self, constraints: &mut GenericBaseConstraints<'db>, origin: StaticClassLiteral<'db>, parameter_index: usize, current: GenericBaseConstraint<'db>) -> Result<GenericBaseConstraint<'db>, Self::Error>;
        #[operation(local)]
        async fn same_argument(&self, earlier: GenericBaseConstraint<'db>, argument: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn same_base(&self, earlier: GenericBaseConstraint<'db>, base_index: usize) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_conflict(&self, header_range: TextRange, base_nodes: Option<&[ast::Expr]>, base: Type<'db>, origin: StaticClassLiteral<'db>, earlier: GenericBaseConstraint<'db>, later: GenericBaseConstraint<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(report_inconsistent_generic_bases_sync)]
    #[capabilities(effects = GenericBaseCheckEffects)]
    #[passive_values(ClassType::Generic, ClassType::NonGeneric, GenericBaseConstraint)]
    pub(in crate::types) async fn report_inconsistent_generic_bases_with<'db, E: GenericBaseCheckEffects<'db>>(
        header_range: TextRange,
        explicit_bases: &[Type<'db>],
        base_nodes: Option<&[ast::Expr]>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        // Track the first non-dynamic argument at each position, along with the alias and explicit
        // base that supplied it. Compatibility with a gradual argument is not transitive: both
        // `Base[int, str]` and `Base[int, bytes]` are compatible with `Base[int, Any]`, but conflict
        // with each other.
        let mut constraints = effects.empty_constraints().await?;
        let mut base_cursor = 0;
        #[cursor_loop]
        while let Some(indexed_base) = effects.next_type(explicit_bases, &mut base_cursor).await? {
            let (base_index, base) = indexed_base;
            let base_class = match base {
                Type::GenericAlias(alias) => ClassType::Generic(alias),
                Type::ClassLiteral(class) if !effects.has_generic_context(class).await? => {
                    ClassType::NonGeneric(class)
                }
                _ => continue,
            };
            let mut ancestors = effects.ancestors_start(base_class).await?;
            #[cursor_loop]
            while let Some(ancestor) = effects.ancestors_next(&mut ancestors).await? {
                let ClassType::Generic(alias) = ancestor else {
                    continue;
                };
                let origin = effects.origin(alias).await?;
                let arguments = effects.arguments(alias).await?;
                let mut argument_cursor = 0;
                #[cursor_loop]
                while let Some(indexed_argument) = effects.next_type(arguments, &mut argument_cursor).await? {
                    let (parameter_index, argument) = indexed_argument;
                    if effects.is_dynamic(argument).await? {
                        continue;
                    }
                    let later = GenericBaseConstraint { argument, alias, base_index };
                    let earlier = effects.remember_argument(&mut constraints, origin, parameter_index, later).await?;
                    if effects.same_argument(earlier, argument).await? {
                        continue;
                    }
                    if effects.same_base(earlier, base_index).await? {
                        return Ok(true);
                    }
                    effects.report_conflict(header_range, base_nodes, base, origin, earlier, later).await?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

pub(in crate::types) fn next_generic_base_type<'db>(
    types: &[Type<'db>],
    cursor: &mut usize,
) -> Option<(usize, Type<'db>)> {
    let index = *cursor;
    let ty = *types.get(index)?;
    *cursor += 1;
    Some((index, ty))
}

pub(super) struct OrdinaryGenericBaseCheckEffects<'a, 'db, 'ast> {
    context: &'a InferContext<'db, 'ast>,
}

impl<'a, 'db, 'ast> OrdinaryGenericBaseCheckEffects<'a, 'db, 'ast> {
    pub(super) fn new(context: &'a InferContext<'db, 'ast>) -> Self {
        Self { context }
    }
}

impl<'db> SynchronousGenericBaseCheckEffects<'db> for OrdinaryGenericBaseCheckEffects<'_, 'db, '_> {
    type Error = Infallible;
    type Ancestors<'state>
        = ExplicitClassAncestors<'state, 'db>
    where
        Self: 'state;

    fn empty_constraints(&self) -> Result<GenericBaseConstraints<'db>, Infallible> {
        Ok(GenericBaseConstraints::default())
    }

    fn next_type(
        &self,
        types: &[Type<'db>],
        cursor: &mut usize,
    ) -> Result<Option<(usize, Type<'db>)>, Infallible> {
        Ok(next_generic_base_type(types, cursor))
    }

    fn has_generic_context(&self, class: ClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.generic_context(self.context.db()).is_some())
    }

    fn ancestors_start(&self, class: ClassType<'db>) -> Result<Self::Ancestors<'_>, Infallible> {
        Ok(class.iter_explicit_ancestors(self.context.db(), self.context.program_environment()))
    }

    fn ancestors_next<'state>(
        &'state self,
        cursor: &mut Self::Ancestors<'state>,
    ) -> Result<Option<ClassType<'db>>, Infallible> {
        Ok(cursor.next())
    }

    fn origin(&self, alias: GenericAlias<'db>) -> Result<StaticClassLiteral<'db>, Infallible> {
        Ok(alias.origin(self.context.db()))
    }

    fn arguments(&self, alias: GenericAlias<'db>) -> Result<&'db [Type<'db>], Infallible> {
        Ok(alias
            .specialization(self.context.db())
            .types(self.context.db()))
    }

    fn is_dynamic(&self, argument: Type<'db>) -> Result<bool, Infallible> {
        Ok(argument.is_dynamic())
    }

    fn remember_argument(
        &self,
        constraints: &mut GenericBaseConstraints<'db>,
        origin: StaticClassLiteral<'db>,
        parameter_index: usize,
        current: GenericBaseConstraint<'db>,
    ) -> Result<GenericBaseConstraint<'db>, Infallible> {
        Ok(*constraints
            .entry(GenericBaseParameter {
                origin,
                parameter_index,
            })
            .or_insert(current))
    }

    fn same_argument(
        &self,
        earlier: GenericBaseConstraint<'db>,
        argument: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(earlier.has_argument(argument))
    }

    fn same_base(
        &self,
        earlier: GenericBaseConstraint<'db>,
        base_index: usize,
    ) -> Result<bool, Infallible> {
        Ok(earlier.has_base(base_index))
    }

    fn report_conflict(
        &self,
        header_range: TextRange,
        base_nodes: Option<&[ast::Expr]>,
        base: Type<'db>,
        origin: StaticClassLiteral<'db>,
        earlier: GenericBaseConstraint<'db>,
        later: GenericBaseConstraint<'db>,
    ) -> Result<(), Infallible> {
        let context = self.context;
        let db = context.db();
        let env = context.program_environment();
        let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, header_range) else {
            return Ok(());
        };
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Inconsistent type arguments for `{}` among class bases",
            origin.name(db)
        ));
        let later_is_direct = matches!(
            base,
            Type::GenericAlias(alias) if alias.origin(db) == origin
        );

        if let (Some(earlier_base), Some(later_base)) = (
            base_nodes.and_then(|nodes| nodes.get(earlier.base_index)),
            base_nodes.and_then(|nodes| nodes.get(later.base_index)),
        ) {
            diagnostic.annotate(context.secondary(earlier_base).message(format_args!(
                "Earlier class base inherits from `{}`",
                earlier.alias.display(db, env)
            )));
            let later_annotation = context.secondary(later_base);
            diagnostic.annotate(if later_is_direct {
                later_annotation.message(format_args!(
                    "Later class base is `{}`",
                    later.alias.display(db, env)
                ))
            } else {
                later_annotation.message(format_args!(
                    "Later class base inherits from `{}`",
                    later.alias.display(db, env)
                ))
            });
        } else {
            diagnostic.info(format_args!(
                "Earlier class base inherits from `{}`",
                earlier.alias.display(db, env)
            ));
            if later_is_direct {
                diagnostic.info(format_args!(
                    "Later class base is `{}`",
                    later.alias.display(db, env)
                ));
            } else {
                diagnostic.info(format_args!(
                    "Later class base inherits from `{}`",
                    later.alias.display(db, env)
                ));
            }
        }
        diagnostic.set_concise_message(format_args!(
            "Inconsistent type arguments: class cannot inherit from both `{}` and `{}`",
            later.alias.display(db, env),
            earlier.alias.display(db, env)
        ));
        Ok(())
    }
}
