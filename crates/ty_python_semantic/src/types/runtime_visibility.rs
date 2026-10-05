//! Runtime visibility of definitions, shared by ordinary and controlled inference.

use std::convert::Infallible;

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use ty_python_core::definition::{AssignmentDefinitionKind, Definition, DefinitionKind};
use ty_python_core::{ProgramFile, SemanticIndex, semantic_index};

use crate::types::infer::{DefinitionInference, infer_definition_types};
use crate::types::typevar::TypeVarInstance;
use crate::types::{KnownInstanceType, Type};
use crate::{Db, HasType, NameKind, SemanticModel};

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousRuntimeVisibilityEffects)]
pub(in crate::types) trait RuntimeVisibilityEffects<'db> {
    type Error;
    type Source;

    #[operation(source)]
    async fn prepare(&self, definition: Definition<'db>) -> Result<Self::Source, Self::Error>;
    #[operation(local)]
    async fn in_type_checking_block(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
    ) -> Result<bool, Self::Error>;
    #[operation(local)]
    async fn is_stub(&self, source: &Self::Source) -> Result<bool, Self::Error>;
    #[operation(local)]
    async fn is_binding(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
        is_stub: bool,
    ) -> Result<bool, Self::Error>;
    #[operation(child)]
    async fn definition_inference(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionInference<'db>, Self::Error>;
    #[operation(local)]
    async fn binding_type(
        &self,
        inference: &DefinitionInference<'db>,
        definition: Definition<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    #[operation(local)]
    async fn undecorated_type(
        &self,
        inference: &DefinitionInference<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    #[operation(child)]
    async fn is_type_check_only(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
    #[operation(local)]
    async fn is_private(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
    ) -> Result<bool, Self::Error>;
    #[operation(local)]
    async fn is_original_typevar(
        &self,
        typevar: TypeVarInstance<'db>,
        definition: Definition<'db>,
    ) -> Result<bool, Self::Error>;
    #[operation(local)]
    async fn definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error>;
    #[operation(child)]
    async fn is_type_alias_annotation(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
    ) -> Result<bool, Self::Error>;
    #[operation(local)]
    async fn assignment_value<'source>(
        &self,
        assignment: &AssignmentDefinitionKind<'db>,
        source: &'source Self::Source,
    ) -> Result<&'source ast::Expr, Self::Error>;
    #[operation(child)]
    async fn subscript_value_type(
        &self,
        value: &ast::Expr,
        source: &Self::Source,
    ) -> Result<Option<Type<'db>>, Self::Error>;
}

#[synchronous(runtime_visibility_sync)]
#[capabilities(effects = RuntimeVisibilityEffects)]
#[passive_values()]
pub(in crate::types) async fn runtime_visibility_with<'db, E: RuntimeVisibilityEffects<'db>>(
    definition: Definition<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    let source = effects.prepare(definition).await?;

    // Definitions inside an `if TYPE_CHECKING` block are never available at runtime.
    if effects.in_type_checking_block(definition, &source).await? {
        return Ok(false);
    }

    // A declaration (without binding) can describe a value initialized elsewhere, but inference
    // only records its declared type. Treat it as a possible runtime value without querying the
    // type of its binding.
    let is_stub = effects.is_stub(&source).await?;
    if !effects.is_binding(definition, &source, is_stub).await? {
        return Ok(true);
    }

    let inference = effects.definition_inference(definition).await?;
    let ty = effects.binding_type(inference, definition).await?;

    // A class or function decorated with `@type_check_only` never exists at runtime.
    if effects.is_type_check_only(ty).await? {
        return Ok(false);
    }
    if let Some(undecorated) = effects.undecorated_type(inference).await?
        && effects.is_type_check_only(undecorated).await?
    {
        return Ok(false);
    }

    // The remaining heuristics only apply to stub definitions.
    if !is_stub {
        return Ok(true);
    }
    if !effects.is_private(definition, &source).await? {
        return Ok(true);
    }

    // Private type variables, parameter specifications, and type-variable tuples in stubs are
    // implementation details rather than runtime values.
    if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = ty
        && effects.is_original_typevar(typevar, definition).await?
    {
        return Ok(false);
    }

    // Explicit PEP 613 and PEP 695 type aliases in stubs are also typing-only helpers.
    let kind = effects.definition_kind(definition).await?;
    match kind {
        DefinitionKind::TypeAlias(_) => return Ok(false),
        DefinitionKind::AnnotatedAssignment(_) => {
            return Ok(!effects.is_type_alias_annotation(definition, &source).await?);
        }
        _ => {}
    }
    let DefinitionKind::Assignment(assignment) = kind else {
        return Ok(true);
    };

    // Treat only unambiguous union, `Literal`, and `Annotated` expressions as implicit aliases.
    // Other expressions may also be aliases, but a false negative is preferable to incorrectly
    // hiding a value that exists at runtime.
    match (ty, effects.assignment_value(assignment, &source).await?) {
        (
            Type::KnownInstance(KnownInstanceType::UnionType(_)),
            ast::Expr::BinOp(ast::ExprBinOp {
                op: ast::Operator::BitOr,
                ..
            }),
        ) => Ok(false),
        (
            Type::KnownInstance(KnownInstanceType::Literal(_) | KnownInstanceType::Annotated(_)),
            ast::Expr::Subscript(subscript),
        ) => Ok(!matches!(
            effects.subscript_value_type(&subscript.value, &source).await?,
            Some(Type::SpecialForm(_) | Type::ClassLiteral(_) | Type::GenericAlias(_))
        )),
        _ => Ok(true),
    }
}
}

pub(in crate::types) struct InlineRuntimeVisibilityEffects<'db>(pub &'db dyn Db);

pub(in crate::types) struct RuntimeVisibilitySource<'db> {
    file: ProgramFile<'db>,
    module: ParsedModuleRef,
    index: &'db SemanticIndex<'db>,
}

impl<'db> SynchronousRuntimeVisibilityEffects<'db> for InlineRuntimeVisibilityEffects<'db> {
    type Error = Infallible;
    type Source = RuntimeVisibilitySource<'db>;

    fn prepare(&self, definition: Definition<'db>) -> Result<Self::Source, Infallible> {
        let file = definition.program_file(self.0);
        Ok(RuntimeVisibilitySource {
            file,
            module: parsed_module(self.0, file.python_file(self.0)).load(self.0),
            index: semantic_index(self.0, file),
        })
    }

    fn in_type_checking_block(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
    ) -> Result<bool, Infallible> {
        Ok(source.index.is_in_type_checking_block(
            definition.file_scope(self.0),
            definition.full_range(self.0, &source.module).range(),
        ))
    }

    fn is_stub(&self, source: &Self::Source) -> Result<bool, Infallible> {
        Ok(source.file.file(self.0).is_stub(self.0))
    }

    fn is_binding(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
        is_stub: bool,
    ) -> Result<bool, Infallible> {
        Ok(definition
            .kind(self.0)
            .category(is_stub, &source.module)
            .is_binding())
    }

    fn definition_inference(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionInference<'db>, Infallible> {
        Ok(infer_definition_types(self.0, definition))
    }

    fn binding_type(
        &self,
        inference: &DefinitionInference<'db>,
        definition: Definition<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(inference.binding_type(definition))
    }

    fn undecorated_type(
        &self,
        inference: &DefinitionInference<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(inference.undecorated_type())
    }

    fn is_type_check_only(&self, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(ty.is_type_check_only(self.0))
    }

    fn is_private(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
    ) -> Result<bool, Infallible> {
        Ok(definition.place(self.0).as_symbol().is_some_and(|symbol| {
            matches!(
                NameKind::classify(
                    source
                        .index
                        .place_table(definition.file_scope(self.0))
                        .symbol(symbol)
                        .name()
                ),
                NameKind::Sunder
            )
        }))
    }

    fn is_original_typevar(
        &self,
        typevar: TypeVarInstance<'db>,
        definition: Definition<'db>,
    ) -> Result<bool, Infallible> {
        Ok(typevar.definition(self.0) == Some(definition))
    }

    fn definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Infallible> {
        Ok(definition.kind(self.0))
    }

    fn is_type_alias_annotation(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
    ) -> Result<bool, Infallible> {
        Ok(SemanticModel::new(self.0, source.file).is_type_alias_definition(definition))
    }

    fn assignment_value<'source>(
        &self,
        assignment: &AssignmentDefinitionKind<'db>,
        source: &'source Self::Source,
    ) -> Result<&'source ast::Expr, Infallible> {
        Ok(assignment.value(&source.module))
    }

    fn subscript_value_type(
        &self,
        value: &ast::Expr,
        source: &Self::Source,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(value.inferred_type(&SemanticModel::new(self.0, source.file)))
    }
}
