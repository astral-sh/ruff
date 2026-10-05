//! Dependencies of preparing and storing a source binding.
//!
//! Prepared source tables belong to the surrounding definition transaction. Every semantic
//! dependency completes before its result is used, and the transaction owns publication.

use std::convert::Infallible;
use std::future::{Future, ready};

use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::Ranged;
use salsa::execution_probe::FieldRequest;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::place::PlaceExprRef;
use ty_python_core::scope::FileScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{DeclarationsIterator, ImportedFinalCandidatesIterator};

use super::{AddBinding, TypeInferenceBuilder};
use crate::diagnostic::format_enumeration;
use crate::place::{
    DefinedPlace, Place, PlaceAndQualifiers, PlaceFromDeclarationsResult,
    module_type_implicit_global_declaration, place_from_declarations_with_reachability_cache,
};
use crate::reachability::ReachabilityEvaluationCache;
use crate::types::attribute_write::{AssignmentAttributeMembers, assignment_attribute_members};
use crate::types::context::InferContext;
use crate::types::diagnostic::{CONFLICTING_DECLARATIONS, INVALID_ASSIGNMENT};
use crate::types::{Type, TypeContext, TypeQualifiers};
use crate::{Db, ProgramEnvironment};

pub(in crate::types::infer) mod sealed {
    pub(in crate::types::infer) trait Sealed {}
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) enum BindingWriteWork {
    Prepare,
    InspectPreviousBinding,
    ReportConflictingDeclarations { types: usize },
    StoreBinding { existing: usize },
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) enum BindingWriteOperation {
    ConflictingDeclarations,
    FinalReassignment,
    AssignmentValidation,
}

pub(in crate::types::infer) trait SourceBindingEffects<'db>:
    sealed::Sealed
{
    type Error;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error>;

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, Self::Error>;

    async fn checkpoint(&self, work: BindingWriteWork) -> Result<(), Self::Error>;

    async fn reachability_cache<'builder>(
        &self,
        builder: &'builder TypeInferenceBuilder<'db, '_>,
    ) -> Result<&'builder ReachabilityEvaluationCache<'db>, Self::Error> {
        Ok(builder.reachability_cache())
    }

    async fn store_binding(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        binding: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        self.checkpoint(BindingWriteWork::StoreBinding {
            existing: builder.bindings.len(),
        })
        .await?;
        builder.bindings.insert(binding, ty);
        Ok(())
    }

    async fn legacy_operation<T>(
        &self,
        operation: BindingWriteOperation,
        body: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn declarations(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'_, 'db>,
        cache: &ReachabilityEvaluationCache<'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error>;

    async fn imported_final(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declared: PlaceFromDeclarationsResult<'db>,
        candidates: ImportedFinalCandidatesIterator<'_, 'db>,
        cache: &ReachabilityEvaluationCache<'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error>;

    async fn forwarded_assignment_owner(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: FileScopeId,
        symbol: ScopedSymbolId,
    ) -> Result<Option<(FileScopeId, ScopedSymbolId)>, Self::Error>;

    async fn implicit_module_declaration(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        prior: PlaceAndQualifiers<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

    async fn fallback_member_declared_type(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        node: AnyNodeRef<'_>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn validate_assignment(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        node: AnyNodeRef<'_>,
        binding: Definition<'db>,
        declaration: Option<Definition<'db>>,
        target: Type<'db>,
        value: Type<'db>,
    ) -> Result<bool, Self::Error>;

    async fn attribute_assignment_transforms_value(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        value: &ast::Expr,
        attribute: &str,
    ) -> Result<bool, Self::Error>;

    async fn safe_subscript_assignment(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        value: &ast::Expr,
    ) -> Result<bool, Self::Error>;
}

pub(in crate::types::infer) struct LegacySourceBindingEffects;

impl sealed::Sealed for LegacySourceBindingEffects {}

impl<'db> SourceBindingEffects<'db> for LegacySourceBindingEffects {
    type Error = Infallible;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, Self::Error> {
        Ok(context.in_stub())
    }

    fn checkpoint(&self, _work: BindingWriteWork) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Ok(()))
    }

    fn legacy_operation<T>(
        &self,
        _operation: BindingWriteOperation,
        body: impl FnOnce() -> T,
    ) -> impl Future<Output = Result<T, Self::Error>> {
        ready(Ok(body()))
    }

    fn declarations(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'_, 'db>,
        cache: &ReachabilityEvaluationCache<'db>,
    ) -> impl Future<Output = Result<PlaceFromDeclarationsResult<'db>, Self::Error>> {
        ready(Ok(place_from_declarations_with_reachability_cache(
            db,
            env,
            declarations,
            cache,
        )))
    }

    fn imported_final(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declared: PlaceFromDeclarationsResult<'db>,
        candidates: ImportedFinalCandidatesIterator<'_, 'db>,
        cache: &ReachabilityEvaluationCache<'db>,
    ) -> impl Future<Output = Result<PlaceFromDeclarationsResult<'db>, Self::Error>> {
        ready(Ok(declared.with_imported_final_for_assignment(
            db, env, candidates, cache,
        )))
    }

    fn forwarded_assignment_owner(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: FileScopeId,
        symbol: ScopedSymbolId,
    ) -> impl Future<Output = Result<Option<(FileScopeId, ScopedSymbolId)>, Self::Error>> {
        ready(Ok(builder.forwarded_assignment_owner(scope, symbol)))
    }

    fn implicit_module_declaration(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        prior: PlaceAndQualifiers<'db>,
        name: &str,
    ) -> impl Future<Output = Result<PlaceAndQualifiers<'db>, Self::Error>> {
        let db = builder.db();
        let env = builder.program_environment();
        ready(Ok(prior.or_fall_back_to(db, env, || {
            module_type_implicit_global_declaration(db, env, name)
        })))
    }

    fn fallback_member_declared_type(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        node: AnyNodeRef<'_>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(Ok(builder.fallback_member_declared_type(node)))
    }

    async fn validate_assignment(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        node: AnyNodeRef<'_>,
        binding: Definition<'db>,
        declaration: Option<Definition<'db>>,
        target: Type<'db>,
        value: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(builder.validate_assignment_type_legacy(node, binding, declaration, target, value))
    }

    fn attribute_assignment_transforms_value(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        value: &ast::Expr,
        attribute: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let db = builder.db();
        let env = builder.program_environment();
        let value_ty = builder.try_expression_type(value).unwrap_or_else(|| {
            builder.infer_maybe_standalone_expression(value, TypeContext::default())
        });
        // Arbitrary data descriptors can transform the assigned value, but slot descriptors
        // write it directly into instance storage.
        ready(Ok(assignment_attribute_members(
            db, env, value_ty, attribute,
        )
        .and_then(AssignmentAttributeMembers::type_member)
        .and_then(|member| member.place.ignore_possibly_undefined())
        .is_some_and(|ty| {
            ty.may_be_data_descriptor(db, env) && !matches!(ty, Type::SlotDescriptor(_))
        })))
    }

    fn safe_subscript_assignment(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        value: &ast::Expr,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let db = builder.db();
        let env = builder.program_environment();
        let value_ty = builder
            .try_expression_type(value)
            .unwrap_or_else(|| builder.infer_expression(value, TypeContext::default()));
        ready(Ok(
            value_ty.is_typed_dict() || AddBinding::is_safe_mutable_class(db, env, value_ty)
        ))
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn validate_assignment_type(
        &self,
        node: AnyNodeRef<'_>,
        binding: Definition<'db>,
        declaration: Option<Definition<'db>>,
        target: Type<'db>,
        value: Type<'db>,
    ) -> bool {
        crate::types::signatures::effects::legacy_inline(self.validate_assignment_type_with(
            &LegacySourceBindingEffects,
            node,
            binding,
            declaration,
            target,
            value,
        ))
    }

    pub(in crate::types::infer) async fn validate_assignment_type_with<
        E: SourceBindingEffects<'db>,
    >(
        &self,
        effects: &E,
        node: AnyNodeRef<'_>,
        binding: Definition<'db>,
        declaration: Option<Definition<'db>>,
        target: Type<'db>,
        value: Type<'db>,
    ) -> Result<bool, E::Error> {
        effects
            .validate_assignment(self, node, binding, declaration, target, value)
            .await
    }

    pub(in crate::types::infer) async fn add_binding_with<'node, E: SourceBindingEffects<'db>>(
        &mut self,
        effects: &E,
        node: AnyNodeRef<'node>,
        binding: Definition<'db>,
    ) -> Result<AddBinding<'db, 'node>, E::Error> {
        effects.checkpoint(BindingWriteWork::Prepare).await?;
        let db = self.db();
        debug_assert!(
            effects
                .field(binding.read_fields(db).kind())
                .await?
                .category(effects.in_stub(&self.context).await?, self.module())
                .is_binding()
        );

        let scope = effects.field(binding.read_fields(db).scope_id()).await?;
        let file_scope_id = effects.field(scope.read_fields(db).file_scope_id()).await?;
        let index = self.index;
        let place_table = index.place_table(file_scope_id);
        let use_def = index.use_def_map(file_scope_id);
        let place_id = effects
            .field(binding.read_fields(db).place_info())
            .await?
            .place();
        let place = place_table.place(place_id);

        let forwarded_owner = if let Some(symbol) = place_id.as_symbol() {
            effects
                .forwarded_assignment_owner(self, file_scope_id, symbol)
                .await?
        } else {
            None
        };
        let (declarations, imported_final_candidates, is_local) =
            if let Some((owner_scope, owner_symbol)) = forwarded_owner {
                let owner_use_def = index.use_def_map(owner_scope);
                (
                    owner_use_def.end_of_scope_symbol_declarations(owner_symbol),
                    owner_use_def.end_of_scope_imported_final_candidates(owner_symbol.into()),
                    false,
                )
            } else {
                (
                    use_def.declarations_at_binding(binding),
                    use_def.imported_final_candidates_at_binding(binding),
                    true,
                )
            };

        let env = self.program_environment();
        let is_import = effects
            .field(binding.read_fields(db).kind())
            .await?
            .is_import();
        let cache = effects.reachability_cache(self).await?;
        let mut declared = effects.declarations(db, env, declarations, cache).await?;
        let mut has_final_declaration = declared.qualifiers().contains(TypeQualifiers::FINAL);
        if !is_import {
            declared = effects
                .imported_final(db, env, declared, imported_final_candidates, cache)
                .await?;
        }
        let (mut place_and_quals, conflicting) = declared.into_place_and_conflicting_declarations();

        // Imports into `global` and `nonlocal` names retain their qualifiers in the forwarding
        // scope, while the owner's declarations continue to supply the declared type.
        if !is_local && !is_import {
            let local_declared = effects
                .declarations(db, env, use_def.declarations_at_binding(binding), cache)
                .await?;
            has_final_declaration |= local_declared.qualifiers().contains(TypeQualifiers::FINAL);
            let local_place = effects
                .imported_final(
                    db,
                    env,
                    local_declared,
                    use_def.imported_final_candidates_at_binding(binding),
                    cache,
                )
                .await?
                .ignore_conflicting_declarations();

            place_and_quals.qualifiers |= local_place.qualifiers;
            if place_and_quals.place.is_undefined() {
                place_and_quals.place = local_place.place;
            }
        }

        if let Some(conflicting) = conflicting {
            effects
                .checkpoint(BindingWriteWork::ReportConflictingDeclarations {
                    types: conflicting.len(),
                })
                .await?;
            effects
                .legacy_operation(BindingWriteOperation::ConflictingDeclarations, || {
                    // TODO point out the conflicting declarations in the diagnostic?
                    let place = place_table.place(binding.place(db));
                    if let Some(builder) = self.context.report_lint(&CONFLICTING_DECLARATIONS, node)
                    {
                        builder.into_diagnostic(format_args!(
                            "Conflicting declared types for `{place}`: {}",
                            format_enumeration(conflicting.iter().map(|ty| ty.display(db, env)))
                        ));
                    }
                })
                .await?;
        }

        // Fall back to implicit module globals for (possibly) unbound names.
        if !place_and_quals.place.is_definitely_bound()
            && let PlaceExprRef::Symbol(symbol) = place
        {
            let symbol_id = place_id.expect_symbol();
            if self.skip_non_global_scopes(file_scope_id, symbol_id)
                || effects
                    .field(self.scope.read_fields(db).file_scope_id())
                    .await?
                    .is_global()
            {
                place_and_quals = effects
                    .implicit_module_declaration(self, place_and_quals, symbol.name())
                    .await?;
            }
        }

        let PlaceAndQualifiers {
            place: resolved_place,
            qualifiers,
        } = place_and_quals;
        let declaration = match resolved_place {
            Place::Defined(DefinedPlace { provenance, .. }) => {
                if let Some(declaration) = provenance.definition() {
                    let scope = effects
                        .field(declaration.read_fields(db).scope_id())
                        .await?;
                    let file = effects.field(scope.read_fields(db).program_file()).await?;
                    let python_file = effects.field(file.read_fields(db).python_file()).await?;
                    let file = effects.field(python_file.read_fields(db).file()).await?;
                    (file == self.context.file()).then_some(declaration)
                } else {
                    None
                }
            }
            Place::Undefined => None,
        };
        let declared_ty = if resolved_place.is_undefined() && !place.is_symbol() {
            effects.fallback_member_declared_type(self, node).await?
        } else {
            None
        }
        .or_else(|| resolved_place.ignore_possibly_undefined());

        Ok(AddBinding {
            declared_ty,
            declaration,
            binding,
            node,
            qualifiers,
            is_local,
            has_final_declaration,
        })
    }
}

impl<'db, 'ast> AddBinding<'db, 'ast> {
    pub(in crate::types::infer) async fn insert_with<E: SourceBindingEffects<'db>>(
        self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        effects: &E,
        inferred_ty: Type<'db>,
    ) -> Result<Type<'db>, E::Error> {
        let declared_ty = self.declared_ty.unwrap_or(Type::unknown());
        let db = builder.db();
        let scope = effects
            .field(self.binding.read_fields(db).scope_id())
            .await?;
        let file_scope_id = effects.field(scope.read_fields(db).file_scope_id()).await?;
        let index = builder.index;
        let use_def = index.use_def_map(file_scope_id);
        let place_table = index.place_table(file_scope_id);
        let mut bound_ty = inferred_ty;

        if self.qualifiers.contains(TypeQualifiers::FINAL) {
            // An assignment to a local `Final`-qualified symbol is only an error if there are prior bindings.
            let mut previous_definition = None;
            for previous in use_def.bindings_at_definition(self.binding) {
                effects
                    .checkpoint(BindingWriteWork::InspectPreviousBinding)
                    .await?;
                if let Some(definition) = previous.binding.definition() {
                    previous_definition = Some(definition);
                    break;
                }
            }

            if !self.is_local || previous_definition.is_some() {
                effects
                    .legacy_operation(BindingWriteOperation::FinalReassignment, || {
                        let place = place_table.place(self.binding.place(db));
                        if let Some(diag_builder) = builder.context.report_lint(
                            &INVALID_ASSIGNMENT,
                            self.binding.full_range(db, builder.module()),
                        ) {
                            let mut diagnostic = diag_builder.into_diagnostic(format_args!(
                                "Reassignment of `Final` symbol `{place}` is not allowed"
                            ));
                            diagnostic
                                .set_primary_annotation_message("Reassignment of `Final` symbol");
                            if self.has_final_declaration
                                && let Some(previous_definition) = previous_definition
                                && !previous_definition.kind(db).is_import()
                            {
                                // Imported `Final` has no local declaration to point to: an earlier invalid
                                // assignment is not its declaration. Ideally, we would show the original
                                // definition in the external module.
                                let annotation =
                                    if let DefinitionKind::AnnotatedAssignment(assignment) =
                                        previous_definition.kind(db)
                                    {
                                        builder.context.secondary(
                                            assignment.annotation(builder.module()).range(),
                                        )
                                    } else {
                                        builder.context.secondary(
                                            previous_definition.full_range(db, builder.module()),
                                        )
                                    };
                                diagnostic.annotate(
                                    annotation.message("Symbol declared as `Final` here"),
                                );
                                diagnostic
                                    .set_primary_annotation_message("Symbol later reassigned here");
                            }
                        }
                    })
                    .await?;
            }
        }

        if !effects
            .validate_assignment(
                builder,
                self.node,
                self.binding,
                self.declaration,
                declared_ty,
                bound_ty,
            )
            .await?
        {
            builder.discard_dict_key_assignments_for(self.binding);
            // Allow declarations to override inference in case of invalid assignment.
            bound_ty = declared_ty;
        }

        // Data descriptors and arbitrary subscript implementations can transform assigned values.
        if let AnyNodeRef::ExprAttribute(ast::ExprAttribute { value, attr, .. }) = self.node
            && effects
                .attribute_assignment_transforms_value(builder, value, &attr.id)
                .await?
        {
            builder.discard_dict_key_assignments_for(self.binding);
            bound_ty = declared_ty;
        } else if let AnyNodeRef::ExprSubscript(ast::ExprSubscript { value, .. }) = self.node
            && !effects.safe_subscript_assignment(builder, value).await?
        {
            builder.discard_dict_key_assignments_for(self.binding);
            bound_ty = declared_ty;
        }

        effects
            .store_binding(builder, self.binding, bound_ty)
            .await?;
        Ok(inferred_ty)
    }
}
