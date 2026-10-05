//! Source-ordered expansion of explicit class bases.

use std::borrow::Cow;
use std::convert::Infallible;

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast as ast;
use ty_python_core::definition::Definition;
use ty_python_core::scope::{FileScopeId, ScopeId};
use ty_python_core::{SemanticIndex, semantic_index};

use super::source::{SourceClassEffects, SourceClassError};
use super::static_literal::ExpandedClassBaseEntry;
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::tuple::{Tuple, TupleSpec};
use crate::types::{KnownClass, StaticClassLiteral, Type, definition_expression_type};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ClassBaseEntryWork {
    Owner,
    Begin,
    Capacity { prefix_len: usize, capacity: usize },
    Base { index: usize },
    Expression,
    TupleSpec,
    TupleSource { index: usize },
    TupleElement { index: usize },
    Append { prefix_len: usize },
    Publish { len: usize },
    BoxOutput { len: usize },
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

pub(in crate::types) trait ClassBaseEntryEffects<'db>: sealed::Sealed {
    type Error;

    fn checkpoint(&self, work: ClassBaseEntryWork) -> Result<(), Self::Error>;

    fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error>;

    /// Tuple lookup can itself require semantic work; providers must admit its dependencies
    /// before returning either an owned or borrowed tuple specification.
    fn tuple_spec(
        &self,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Self::Error>;
}

pub(in crate::types) fn expanded_class_base_entries_with<'a, 'db, E: ClassBaseEntryEffects<'db>>(
    known_class: Option<KnownClass>,
    class_stmt: &'a ast::StmtClassDef,
    class_definition: Definition<'db>,
    effects: &E,
) -> Result<Vec<ExpandedClassBaseEntry<'a, 'db>>, E::Error> {
    expanded_class_base_entries_sync(
        known_class,
        class_stmt,
        class_definition,
        ClassBaseEntryFacts,
        &SynchronousBaseEntryAdapter(effects),
    )
}

pub(in crate::types) struct ClassBaseCursor<'a> {
    entries: std::iter::Enumerate<std::slice::Iter<'a, ast::Expr>>,
}

impl<'a> ClassBaseCursor<'a> {
    pub(in crate::types) fn next(&mut self) -> Option<(usize, &'a ast::Expr)> {
        self.entries.next()
    }

    fn index(&self) -> Option<usize> {
        self.entries.clone().next().map(|(index, _)| index)
    }
}

pub(in crate::types) struct ClassTupleCursor<'a, 'db> {
    entries: std::iter::Enumerate<std::slice::Iter<'a, Type<'db>>>,
}

impl<'a, 'db> ClassTupleCursor<'a, 'db> {
    pub(in crate::types) fn next(&mut self) -> Option<(usize, Type<'db>)> {
        self.entries.next().map(|(index, ty)| (index, *ty))
    }

    fn index(&self) -> Option<usize> {
        self.entries.clone().next().map(|(index, _)| index)
    }
}

pub(in crate::types) struct ClassBaseEntryFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousBaseEntryDriverEffects)]
    pub(in crate::types) trait BaseEntryDriverEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: ClassBaseEntryWork) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn empty_entries<'a>(&self) -> Result<Vec<ExpandedClassBaseEntry<'a, 'db>>, Self::Error>;
        #[operation(local)]
        async fn entries<'a>(&self, capacity: usize) -> Result<Vec<ExpandedClassBaseEntry<'a, 'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_base<'a>(&self, cursor: &mut ClassBaseCursor<'a>) -> Result<Option<(usize, &'a ast::Expr)>, Self::Error>;
        #[operation(child)]
        async fn expression_type(&self, definition: Definition<'db>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn tuple_spec(&self, definition: Definition<'db>, ty: Type<'db>) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_source<'a>(&self, cursor: &mut ClassBaseCursor<'a>) -> Result<Option<(usize, &'a ast::Expr)>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_tuple<'a>(&self, cursor: &mut ClassTupleCursor<'a, 'db>) -> Result<Option<(usize, Type<'db>)>, Self::Error>;
        #[operation(local)]
        async fn append_entry<'a>(&self, entries: &mut Vec<ExpandedClassBaseEntry<'a, 'db>>, source_node: &'a ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn publish_entries<'a>(&self, entries: Vec<ExpandedClassBaseEntry<'a, 'db>>) -> Result<Vec<ExpandedClassBaseEntry<'a, 'db>>, Self::Error>;
    }

    #[finite_capability]
    impl ClassBaseEntryFacts {
        fn not_implemented(&self, known: Option<KnownClass>) -> bool { known == Some(KnownClass::NotImplementedType) }
        fn base_count(&self, class: &ast::StmtClassDef) -> usize { class.bases().len() }
        fn bases<'a>(&self, class: &'a ast::StmtClassDef) -> ClassBaseCursor<'a> {
            ClassBaseCursor { entries: class.bases().iter().enumerate() }
        }
        fn fixed_tuple<'a, 'db>(&self, spec: &'a Option<Cow<'db, TupleSpec<'db>>>) -> Option<&'a [Type<'db>]> {
            match spec.as_deref() { Some(Tuple::Fixed(tuple)) => Some(tuple.elements_slice()), _ => None }
        }
        fn source_elements<'a>(&self, expression: &'a ast::Expr, elements: &[Type<'_>]) -> Option<&'a ast::ExprTuple> {
            expression.as_tuple_expr().filter(|literal| literal.len() == elements.len())
        }
        fn source_cursor<'a>(&self, literal: &'a ast::ExprTuple) -> ClassBaseCursor<'a> {
            ClassBaseCursor { entries: literal.elts.iter().enumerate() }
        }
        fn tuple_cursor<'a, 'db>(&self, elements: &'a [Type<'db>]) -> ClassTupleCursor<'a, 'db> {
            ClassTupleCursor { entries: elements.iter().enumerate() }
        }
        fn is_starred(&self, expression: &ast::Expr) -> bool { expression.is_starred_expr() }
        fn source_node<'a>(&self, literal: Option<&'a ast::ExprTuple>, fallback: &'a ast::Expr, index: usize) -> &'a ast::Expr {
            literal.map_or(fallback, |literal| &literal.elts[index])
        }
        fn plain_source<'a>(&self, plain: bool, literal: &'a ast::ExprTuple) -> Option<&'a ast::ExprTuple> {
            plain.then_some(literal)
        }
    }

    #[synchronous(expanded_class_base_entries_sync)]
    #[capabilities(effects = BaseEntryDriverEffects, facts = ClassBaseEntryFacts)]
    #[passive_values(ClassBaseEntryWork::Begin, ClassBaseEntryWork::Expression, ClassBaseEntryWork::TupleSpec, Type::unknown)]
    pub(in crate::types) async fn expanded_class_base_entries_async_with<'a, 'db, E: BaseEntryDriverEffects<'db>>(
        known_class: Option<KnownClass>,
        class_stmt: &'a ast::StmtClassDef,
        class_definition: Definition<'db>,
        facts: ClassBaseEntryFacts,
        effects: &E,
    ) -> Result<Vec<ExpandedClassBaseEntry<'a, 'db>>, E::Error> {
        effects.checkpoint(ClassBaseEntryWork::Begin).await?;
        // Typeshed gives NotImplementedType an Any base, which would make its instances gradual.
        if facts.not_implemented(known_class) {
            let entries = effects.empty_entries().await?;
            return effects.publish_entries(entries).await;
        }
        let mut entries = effects.entries(facts.base_count(class_stmt)).await?;
        let mut bases = facts.bases(class_stmt);
        #[cursor_loop]
        while let Some(entry) = effects.next_base(&mut bases).await? {
            let (_, base_node) = entry;
            let ty = if let ast::Expr::Starred(starred) = base_node {
                effects.checkpoint(ClassBaseEntryWork::Expression).await?;
                let starred_ty = effects.expression_type(class_definition, &starred.value).await?;
                effects.checkpoint(ClassBaseEntryWork::TupleSpec).await?;
                let spec = effects.tuple_spec(class_definition, starred_ty).await?;
                if let Some(elements) = facts.fixed_tuple(&spec) {
                    let source_elements = if let Some(literal) = facts.source_elements(&starred.value, elements) {
                        #[passive_state]
                        let mut plain = true;
                        let mut source = facts.source_cursor(literal);
                        #[cursor_loop]
                        while let Some(source_entry) = effects.next_source(&mut source).await? {
                            let (_, element) = source_entry;
                            if facts.is_starred(element) {
                                plain = false;
                                break;
                            }
                        }
                        facts.plain_source(plain, literal)
                    } else {
                        None
                    };
                    let mut tuple = facts.tuple_cursor(elements);
                    #[cursor_loop]
                    while let Some(tuple_entry) = effects.next_tuple(&mut tuple).await? {
                        let (index, ty) = tuple_entry;
                        effects.append_entry(&mut entries, facts.source_node(source_elements, base_node, index), ty).await?;
                    }
                    continue;
                }
                Type::unknown()
            } else {
                effects.checkpoint(ClassBaseEntryWork::Expression).await?;
                effects.expression_type(class_definition, base_node).await?
            };
            effects.append_entry(&mut entries, base_node, ty).await?;
        }
        effects.publish_entries(entries).await
    }
}

struct SynchronousBaseEntryAdapter<'effects, E>(&'effects E);

impl<'db, E: ClassBaseEntryEffects<'db>> SynchronousBaseEntryDriverEffects<'db>
    for SynchronousBaseEntryAdapter<'_, E>
{
    type Error = E::Error;

    fn checkpoint(&self, work: ClassBaseEntryWork) -> Result<(), E::Error> {
        self.0.checkpoint(work)
    }
    fn empty_entries<'a>(&self) -> Result<Vec<ExpandedClassBaseEntry<'a, 'db>>, E::Error> {
        Ok(Vec::new())
    }
    fn entries<'a>(
        &self,
        capacity: usize,
    ) -> Result<Vec<ExpandedClassBaseEntry<'a, 'db>>, E::Error> {
        self.0.checkpoint(ClassBaseEntryWork::Capacity {
            prefix_len: 0,
            capacity,
        })?;
        Ok(Vec::with_capacity(capacity))
    }
    fn next_base<'a>(
        &self,
        cursor: &mut ClassBaseCursor<'a>,
    ) -> Result<Option<(usize, &'a ast::Expr)>, E::Error> {
        if let Some(index) = cursor.index() {
            self.0.checkpoint(ClassBaseEntryWork::Base { index })?;
        }
        Ok(cursor.next())
    }
    fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, E::Error> {
        self.0.expression_type(definition, expression)
    }
    fn tuple_spec(
        &self,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<Option<Cow<'db, TupleSpec<'db>>>, E::Error> {
        self.0.tuple_spec(definition, ty)
    }
    fn next_source<'a>(
        &self,
        cursor: &mut ClassBaseCursor<'a>,
    ) -> Result<Option<(usize, &'a ast::Expr)>, E::Error> {
        if let Some(index) = cursor.index() {
            self.0
                .checkpoint(ClassBaseEntryWork::TupleSource { index })?;
        }
        Ok(cursor.next())
    }
    fn next_tuple<'a>(
        &self,
        cursor: &mut ClassTupleCursor<'a, 'db>,
    ) -> Result<Option<(usize, Type<'db>)>, E::Error> {
        if let Some(index) = cursor.index() {
            self.0
                .checkpoint(ClassBaseEntryWork::TupleElement { index })?;
        }
        Ok(cursor.next())
    }
    fn append_entry<'a>(
        &self,
        entries: &mut Vec<ExpandedClassBaseEntry<'a, 'db>>,
        source_node: &'a ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), E::Error> {
        append_entry(entries, ExpandedClassBaseEntry { source_node, ty }, self.0)
    }
    fn publish_entries<'a>(
        &self,
        entries: Vec<ExpandedClassBaseEntry<'a, 'db>>,
    ) -> Result<Vec<ExpandedClassBaseEntry<'a, 'db>>, E::Error> {
        self.0
            .checkpoint(ClassBaseEntryWork::Publish { len: entries.len() })?;
        Ok(entries)
    }
}

/// The parsed module owns every expression borrowed by the base-expansion driver.
pub(in crate::types) struct ClassBaseSource<'db> {
    pub(in crate::types) module: ParsedModuleRef,
    pub(in crate::types) index: &'db SemanticIndex<'db>,
    pub(in crate::types) scope: ScopeId<'db>,
    pub(in crate::types) definition: Definition<'db>,
    pub(in crate::types) known: Option<KnownClass>,
}

impl<'db> ClassBaseSource<'db> {
    pub(in crate::types) fn node<'a>(&'a self, db: &'db dyn Db) -> &'a ast::StmtClassDef {
        self.node_in_scope(self.scope.file_scope_id(db))
    }

    pub(in crate::types) fn node_in_scope(&self, file_scope: FileScopeId) -> &ast::StmtClassDef {
        self.index
            .scope(file_scope)
            .node()
            .expect_class()
            .node(&self.module)
    }
}

pub(in crate::types) struct ClassBaseTypeCursor<'a, 'db> {
    entries: std::vec::IntoIter<ExpandedClassBaseEntry<'a, 'db>>,
}

impl<'db> ClassBaseTypeCursor<'_, 'db> {
    pub(in crate::types) fn next(&mut self) -> Option<Type<'db>> {
        self.entries.next().map(ExpandedClassBaseEntry::ty)
    }
}

pub(in crate::types) struct ExplicitBaseFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousExplicitBaseEffects)]
    pub(in crate::types) trait ExplicitBaseEffects<'db> {
        type Error;

        #[operation(source)]
        async fn source(&self, class: StaticClassLiteral<'db>) -> Result<ClassBaseSource<'db>, Self::Error>;
        #[operation(child)]
        async fn expand<'a>(&self, source: &'a ClassBaseSource<'db>) -> Result<Vec<ExpandedClassBaseEntry<'a, 'db>>, Self::Error>;
        #[operation(local)]
        async fn type_buffer(&self, capacity: usize) -> Result<Vec<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn type_cursor<'a>(&self, entries: Vec<ExpandedClassBaseEntry<'a, 'db>>) -> Result<ClassBaseTypeCursor<'a, 'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type(&self, cursor: &mut ClassBaseTypeCursor<'_, 'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn append_type(&self, types: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn box_types(&self, types: Vec<Type<'db>>) -> Result<Box<[Type<'db>]>, Self::Error>;
        #[operation(local)]
        async fn publish_types(&self, types: Box<[Type<'db>]>) -> Result<Box<[Type<'db>]>, Self::Error>;
        #[operation(local)]
        async fn seed(&self, source: &ClassBaseSource<'db>, id: salsa::Id) -> Result<Box<[Type<'db>]>, Self::Error>;
        #[operation(checkpoint)]
        async fn recovery_checkpoint(&self) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn normalize(&self, cycle: &salsa::Cycle<'_>, previous: &[Type<'db>], current: Box<[Type<'db>]>, class: StaticClassLiteral<'db>) -> Result<Box<[Type<'db>]>, Self::Error>;
    }

    #[finite_capability]
    impl ExplicitBaseFacts {
        fn entries_len(&self, entries: &[ExpandedClassBaseEntry<'_, '_>]) -> usize { entries.len() }
        fn same_length(&self, previous: &[Type<'_>], current: &[Type<'_>]) -> bool { previous.len() == current.len() }
    }

    #[synchronous(explicit_base_types_sync)]
    #[capabilities(effects = ExplicitBaseEffects, facts = ExplicitBaseFacts)]
    #[passive_values()]
    pub(in crate::types) async fn explicit_base_types_async_with<'db, E: ExplicitBaseEffects<'db>>(
        class: StaticClassLiteral<'db>, facts: ExplicitBaseFacts, effects: &E,
    ) -> Result<Box<[Type<'db>]>, E::Error> {
        let source = effects.source(class).await?;
        let entries = effects.expand(&source).await?;
        let mut types = effects.type_buffer(facts.entries_len(&entries)).await?;
        let mut cursor = effects.type_cursor(entries).await?;
        #[cursor_loop]
        while let Some(ty) = effects.next_type(&mut cursor).await? {
            effects.append_type(&mut types, ty).await?;
        }
        let types = effects.box_types(types).await?;
        effects.publish_types(types).await
    }

    #[synchronous(initial_explicit_base_types_sync)]
    #[capabilities(effects = ExplicitBaseEffects)]
    #[passive_values()]
    pub(in crate::types) async fn initial_explicit_base_types_with<'db, E: ExplicitBaseEffects<'db>>(
        id: salsa::Id, class: StaticClassLiteral<'db>, effects: &E,
    ) -> Result<Box<[Type<'db>]>, E::Error> {
        let source = effects.source(class).await?;
        effects.seed(&source, id).await
    }

    #[synchronous(recover_explicit_base_types_sync)]
    #[capabilities(effects = ExplicitBaseEffects, facts = ExplicitBaseFacts)]
    #[passive_values()]
    pub(in crate::types) async fn recover_explicit_base_types_with<'db, E: ExplicitBaseEffects<'db>>(
        cycle: &salsa::Cycle<'_>, previous: &[Type<'db>], current: Box<[Type<'db>]>,
        class: StaticClassLiteral<'db>, facts: ExplicitBaseFacts, effects: &E,
    ) -> Result<Box<[Type<'db>]>, E::Error> {
        if !effects.recovery_checkpoint().await? {
            return Ok(current);
        }
        if facts.same_length(previous, &current) {
            // Equal lengths retain corresponding bases, so normalize each current type against
            // its previous value. Starred expansion can change the length and that correspondence.
            effects.normalize(cycle, previous, current, class).await
        } else {
            Ok(current)
        }
    }
}

pub(in crate::types) fn type_cursor<'a, 'db>(
    entries: Vec<ExpandedClassBaseEntry<'a, 'db>>,
) -> ClassBaseTypeCursor<'a, 'db> {
    ClassBaseTypeCursor {
        entries: entries.into_iter(),
    }
}

pub(in crate::types) fn base_entry<'a, 'db>(
    source_node: &'a ast::Expr,
    ty: Type<'db>,
) -> ExpandedClassBaseEntry<'a, 'db> {
    ExpandedClassBaseEntry { source_node, ty }
}

pub(in crate::types) struct InlineExplicitBaseEffects<'db, 'effects, E> {
    db: &'db dyn Db,
    effects: &'effects E,
}

impl<'db, 'effects, E> InlineExplicitBaseEffects<'db, 'effects, E> {
    pub(in crate::types) fn new(db: &'db dyn Db, effects: &'effects E) -> Self {
        Self { db, effects }
    }
}

impl<'db, E> SynchronousExplicitBaseEffects<'db> for InlineExplicitBaseEffects<'db, '_, E>
where
    E: ClassBaseEntryEffects<'db>
        + SourceReadControl<Error = <E as ClassBaseEntryEffects<'db>>::Error>,
{
    type Error = <E as ClassBaseEntryEffects<'db>>::Error;

    fn source(&self, class: StaticClassLiteral<'db>) -> Result<ClassBaseSource<'db>, Self::Error> {
        self.effects.checkpoint(ClassBaseEntryWork::Owner)?;
        let scope = class.body_scope(self.db);
        let file = scope.program_file(self.db);
        let module = read_source(self.effects, || {
            parsed_module(self.db, file.python_file(self.db)).load(self.db)
        })?;
        let index = read_source(self.effects, || semantic_index(self.db, file))?;
        let definition = index.expect_single_definition(
            index
                .scope(scope.file_scope_id(self.db))
                .node()
                .expect_class(),
        );
        Ok(ClassBaseSource {
            module,
            index,
            scope,
            definition,
            known: class.known(self.db),
        })
    }
    fn expand<'a>(
        &self,
        source: &'a ClassBaseSource<'db>,
    ) -> Result<Vec<ExpandedClassBaseEntry<'a, 'db>>, Self::Error> {
        expanded_class_base_entries_with(
            source.known,
            source.node(self.db),
            source.definition,
            self.effects,
        )
    }
    fn type_buffer(&self, capacity: usize) -> Result<Vec<Type<'db>>, Self::Error> {
        self.effects.checkpoint(ClassBaseEntryWork::Capacity {
            prefix_len: 0,
            capacity,
        })?;
        Ok(Vec::with_capacity(capacity))
    }
    fn type_cursor<'a>(
        &self,
        entries: Vec<ExpandedClassBaseEntry<'a, 'db>>,
    ) -> Result<ClassBaseTypeCursor<'a, 'db>, Self::Error> {
        Ok(type_cursor(entries))
    }
    fn next_type(
        &self,
        cursor: &mut ClassBaseTypeCursor<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(cursor.next())
    }
    fn append_type(&self, types: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Self::Error> {
        self.effects.checkpoint(ClassBaseEntryWork::Append {
            prefix_len: types.len(),
        })?;
        types.push(ty);
        Ok(())
    }
    fn box_types(&self, types: Vec<Type<'db>>) -> Result<Box<[Type<'db>]>, Self::Error> {
        self.effects
            .checkpoint(ClassBaseEntryWork::BoxOutput { len: types.len() })?;
        Ok(types.into_boxed_slice())
    }
    fn publish_types(&self, types: Box<[Type<'db>]>) -> Result<Box<[Type<'db>]>, Self::Error> {
        self.effects
            .checkpoint(ClassBaseEntryWork::Publish { len: types.len() })?;
        Ok(types)
    }
    fn seed(
        &self,
        source: &ClassBaseSource<'db>,
        id: salsa::Id,
    ) -> Result<Box<[Type<'db>]>, Self::Error> {
        // Starred bases can later expand to another length; the initial value uses one entry
        // for each source base until the expression dependencies determine the expanded count.
        read_source(self.effects, || {
            vec![Type::divergent(id); source.node(self.db).bases().len()].into_boxed_slice()
        })
    }
    fn recovery_checkpoint(&self) -> Result<bool, Self::Error> {
        Ok(self.effects.check().is_ok())
    }
    fn normalize(
        &self,
        cycle: &salsa::Cycle<'_>,
        previous: &[Type<'db>],
        current: Box<[Type<'db>]>,
        class: StaticClassLiteral<'db>,
    ) -> Result<Box<[Type<'db>]>, Self::Error> {
        let env = ProgramEnvironment::from_scope(class.body_scope(self.db));
        let normalized = current
            .iter()
            .zip(previous)
            .map(|(current, previous)| {
                read_source(self.effects, || {
                    current.cycle_normalized(self.db, &env, *previous, cycle)
                })
            })
            .collect::<Result<Box<[_]>, _>>();
        let Ok(normalized) = normalized else {
            return Ok(current);
        };
        if self.effects.check().is_err() {
            return Ok(current);
        }
        Ok(normalized)
    }
}

fn append_entry<'a, 'db, E: ClassBaseEntryEffects<'db>>(
    entries: &mut Vec<ExpandedClassBaseEntry<'a, 'db>>,
    entry: ExpandedClassBaseEntry<'a, 'db>,
    effects: &E,
) -> Result<(), E::Error> {
    effects.checkpoint(ClassBaseEntryWork::Append {
        prefix_len: entries.len(),
    })?;
    if entries.len() == entries.capacity() {
        let capacity = entries.capacity().saturating_mul(2).max(1);
        effects.checkpoint(ClassBaseEntryWork::Capacity {
            prefix_len: entries.len(),
            capacity,
        })?;
        entries.reserve_exact(capacity - entries.len());
    }
    entries.push(entry);
    Ok(())
}

pub(in crate::types) struct InlineClassBaseEntryEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineClassBaseEntryEffects<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl sealed::Sealed for InlineClassBaseEntryEffects<'_> {}

impl SourceReadControl for InlineClassBaseEntryEffects<'_> {
    type Error = Infallible;

    fn check(&self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl<'db> ClassBaseEntryEffects<'db> for InlineClassBaseEntryEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self, _work: ClassBaseEntryWork) -> Result<(), Infallible> {
        Ok(())
    }

    fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(definition_expression_type(self.db, definition, expression))
    }

    fn tuple_spec(
        &self,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Infallible> {
        let env = ProgramEnvironment::from_definition(definition);
        Ok(ty.tuple_instance_spec(self.db, &env))
    }
}

impl sealed::Sealed for SourceClassEffects<'_> {}

impl<'db> ClassBaseEntryEffects<'db> for SourceClassEffects<'db> {
    type Error = SourceClassError;

    fn checkpoint(&self, _work: ClassBaseEntryWork) -> Result<(), SourceClassError> {
        self.check()
    }

    fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, SourceClassError> {
        read_source(self, || {
            definition_expression_type(self.db, definition, expression)
        })
    }

    fn tuple_spec(
        &self,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<Option<Cow<'db, TupleSpec<'db>>>, SourceClassError> {
        let env = ProgramEnvironment::from_definition(definition);
        read_source(self, || ty.tuple_instance_spec(self.db, &env))
    }
}
