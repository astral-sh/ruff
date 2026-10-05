//! Admission for legacy-order base selection, name enumeration, and its second primary annotation.

use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticMessage};
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};

use super::{ClassReports, checked};
use crate::types::class::context::explicit_class_bases_with;
use crate::types::context::lint_reporting::LintReportMetadata;
use crate::types::diagnostic::class_generics::{
    OrderTail, OrderVariables, legacy_order_base_range, order_primary_with,
};
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceEffects};
use crate::types::infer::builder::source_definition::controlled::lint_diagnostic_cost::{
    BufferQuotePreparation, annotation_with_ty_span_quote, buffer_quote_preparation,
    prepared_vec_push_quote, unique_diagnostic_mutation_quote, vector_with_capacity_quote,
};
use crate::types::local_transfer::collections::{
    ALIGNMENT_AS_USIZE, CALL_1, CALL_2, CALL_3, CALL_4, CHECK_LANGUAGE_UB,
    CHECKED_MULTIPLY, MAYBE_IS_ALIGNED_AND_NOT_NULL, NONNULL_AS_PTR,
    NONNULL_NEW_UNCHECKED_WORK, POINTER_ACCESS, POINTER_ADD_WORK, POINTER_PRECONDITION,
    RAW_SLICE_POINTER, VECTOR_CAPACITY, VECTOR_LENGTH, checked as checked_quote, event_quote,
};
use crate::types::typevar::TypeVarInstance;
use crate::types::{StaticClassLiteral, Type};

const SLICE_LENGTH: usize = 2 * CALL_1 + 4;
const SLICE_GET: usize = 3 * CALL_2 + SLICE_LENGTH + 8;
const SLICE_SPLIT: usize = CALL_1 + SLICE_LENGTH + CALL_2 + 12;
const OPTION_TAKE: usize = 2 * CALL_1 + 2 * CALL_2 + 10;
const ORDER_VARIABLE_LENGTH: usize = 3 * CALL_1 + SLICE_LENGTH + 10;
const BOX_BORROW: usize = CALL_1 + 2;
const THIN_PADDING: usize = 4 * CALL_2 + 6 * CALL_1 + 20;
const THIN_HEADER: usize = 2 * CALL_1 + NONNULL_AS_PTR + POINTER_PRECONDITION + 4;
const THIN_LENGTH: usize = 2 * CALL_1 + THIN_HEADER + 6;
const THIN_DATA: usize = CALL_1 + THIN_PADDING + THIN_HEADER + 4 * CALL_1 + 2 * CALL_2
    + NONNULL_AS_PTR + POINTER_ADD_WORK + NONNULL_NEW_UNCHECKED_WORK + ALIGNMENT_AS_USIZE + 34;
const SLICE_FROM_RAW: usize = CALL_2 + CHECK_LANGUAGE_UB + CALL_4
    + MAYBE_IS_ALIGNED_AND_NOT_NULL + CALL_2 + RAW_SLICE_POINTER + 24;
const THIN_SLICE: usize = 2 * CALL_1 + THIN_DATA + THIN_LENGTH + SLICE_FROM_RAW + 4;
const CLASS_BASES: usize = CALL_1 + BOX_BORROW + THIN_SLICE + 8;
const EXPR_RANGE: usize = 5 * CALL_1 + 2 * CALL_2 + 14;
const BASE_STEP: usize = SLICE_GET + CALL_2 + 3 * CALL_1 + 24;
const BASE_RANGE: usize = CALL_3 + 2 * CLASS_BASES + 2 * SLICE_LENGTH + SLICE_GET
    + BOX_BORROW + EXPR_RANGE + 8 * CALL_1 + 5 * CALL_2 + 48;
const ORDER_TAIL: usize = CALL_2 + 2 * SLICE_SPLIT + 34;
const ORDER_VARIABLE_STEP: usize = OPTION_TAKE + SLICE_SPLIT + 5 * CALL_1 + 28;
const NAME_BUFFER_PREPARATION: usize = ORDER_VARIABLE_LENGTH + 4 * CALL_2 + 4 * CALL_1 + 24;
const NAME_BUFFER_CONSTRUCTION: usize = ORDER_VARIABLE_LENGTH + 8;
const NAME_CAPACITY_GUARD: usize = VECTOR_LENGTH + VECTOR_CAPACITY + 14;
const VECTOR_SLICE: usize = 2 * CALL_1 + POINTER_ACCESS + CALL_3 + 8;
const ORDER_PRIMARY: usize = 32 + VECTOR_SLICE + 4 * CALL_1;

pub(super) const MESSAGE_PART_WORK: usize = SLICE_GET + SLICE_LENGTH + CHECKED_MULTIPLY
    + 4 * CALL_2 + CALL_1 + 16 + 8 * CALL_1 + 3 * CALL_2 + 64;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassReports<'_, '_, '_, 'run, 'db, '_, A> {
    pub(super) async fn legacy_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        let bytes = size_of::<[(StaticClassLiteral<'db>, &SourceEffects<'_, 'run, 'db, A>); 2]>();
        self.local(16, bytes, || explicit_class_bases_with(class, self.checks.source)).await?.await
    }

    pub(super) async fn legacy_base_step(&self, bases: &[Type<'db>], cursor: &mut usize) -> RunResult<Option<(usize, Type<'db>)>> {
        let (work, bytes) = const { checked_quote(event_quote(BASE_STEP, &[
            size_of::<(&[Type<'_>], usize)>(), size_of::<Option<(usize, Type<'_>)>>(),
            size_of::<(Option<&Type<'_>>, &usize)>(), size_of::<(usize, bool)>(),
        ])) }?;
        // One indexed read, cursor advance, and the shared Generic/Protocol match and loop edge.
        self.local(work, bytes, || {
            let result = bases.get(*cursor).map(|base| (*cursor, *base));
            *cursor += usize::from(result.is_some());
            result
        }).await
    }

    pub(super) async fn legacy_range(&self, node: &ast::StmtClassDef, bases: &[Type<'db>], index: Option<usize>) -> RunResult<TextRange> {
        let (work, bytes) = const { checked_quote(event_quote(BASE_RANGE, &[
            size_of::<(*const (), usize, usize, usize)>(),
            size_of::<(&ast::StmtClassDef, usize, Option<usize>)>(),
            size_of::<(&[ast::Expr], usize)>(), size_of::<RunResult<TextRange>>(),
        ])) }?;
        self.local(work, bytes, || {
            legacy_order_base_range(node, bases.len(), index).map_err(RunError::Contract)
        }).await?
    }

    pub(super) async fn legacy_primary(&self, first: TypeVarInstance<'db>, remaining: &[TypeVarInstance<'db>]) -> RunResult<DiagnosticMessage> {
        let (work, bytes) = const { checked_quote(event_quote(ORDER_PRIMARY, &[
            size_of::<(TypeVarInstance<'_>, &[TypeVarInstance<'_>], &())>(),
            size_of::<(&Vec<&Name>, *const &Name, usize)>(), size_of::<&[&Name]>(),
        ])) }?;
        self.local(work, bytes, || order_primary_with(first, remaining, self)).await?.await
    }

    pub(super) async fn legacy_tail<'a>(&self, first: TypeVarInstance<'db>, remaining: &'a [TypeVarInstance<'db>]) -> RunResult<OrderTail<'a, 'db>> {
        let (work, bytes) = const { checked_quote(event_quote(ORDER_TAIL, &[
            size_of::<(TypeVarInstance<'_>, &[TypeVarInstance<'_>])>(),
            size_of::<OrderTail<'_, '_>>(), size_of::<Option<(&TypeVarInstance<'_>, &[TypeVarInstance<'_>])>>(),
        ])) }?;
        self.local(work, bytes, || OrderTail::new(first, remaining)).await
    }

    /// Reserves the earlier-name buffer once, including backing retirement on interruption.
    pub(super) async fn legacy_name_buffer(&self, variables: &OrderVariables<'_, 'db>) -> RunResult<Vec<&'db Name>> {
        let (work, bytes) = const {
            match buffer_quote_preparation(BufferQuotePreparation::VectorWithCapacity) {
                Ok((work, bytes)) => Ok((work + NAME_BUFFER_PREPARATION, bytes + NAME_BUFFER_PREPARATION * size_of::<RunResult<(usize, usize)>>())),
                Err(error) => Err(error),
            }
        }?;
        let (work, bytes) = self.local(work, bytes, || -> RunResult<(usize, usize)> {
            let (work, bytes) = vector_with_capacity_quote::<&Name>(variables.len())?;
            Ok((checked(work.checked_add(NAME_BUFFER_CONSTRUCTION))?, checked(bytes.checked_add(NAME_BUFFER_CONSTRUCTION * size_of::<(&OrderVariables<'_, 'db>, usize)>()))?))
        }).await??;
        self.local(work, bytes, || Vec::with_capacity(variables.len())).await
    }

    pub(super) async fn legacy_variable_step(&self, variables: &mut OrderVariables<'_, 'db>) -> RunResult<Option<TypeVarInstance<'db>>> {
        let (work, bytes) = const { checked_quote(event_quote(ORDER_VARIABLE_STEP, &[
            size_of::<(&mut Option<TypeVarInstance<'_>>, Option<TypeVarInstance<'_>>)>(),
            size_of::<Option<(&TypeVarInstance<'_>, &[TypeVarInstance<'_>])>>(),
            size_of::<&mut OrderVariables<'_, '_>>(), size_of::<Option<TypeVarInstance<'_>>>(),
        ])) }?;
        self.local(work, bytes, || variables.next()).await
    }

    pub(super) async fn legacy_retain_name(&self, names: &mut Vec<&'db Name>, name: &'db Name) -> RunResult<()> {
        let (work, bytes) = const {
            match prepared_vec_push_quote::<&Name>() {
                Ok((work, bytes)) => Ok((work + NAME_CAPACITY_GUARD, bytes + NAME_CAPACITY_GUARD * size_of::<(&Vec<&Name>, usize)>() + size_of::<[&Name; 3]>())),
                Err(error) => Err(error),
            }
        }?;
        // A pushed reference prepays its passive retirement. The capacity guard prevents a
        // broken enumeration invariant from allocating outside the reservation above.
        self.local(work, bytes, || {
            if names.len() == names.capacity() {
                return Err(RunError::Contract("legacy ordering name buffer exhausted"));
            }
            names.push(name);
            Ok(())
        }).await?
    }

    pub(super) async fn legacy_additional_primary(&self, metadata: &LintReportMetadata, diagnostic: &mut Diagnostic, message: DiagnosticMessage) -> RunResult<()> {
        let (work, bytes) = const {
            match (unique_diagnostic_mutation_quote(), annotation_with_ty_span_quote(), prepared_vec_push_quote::<Annotation>()) {
                (Ok((a, ab)), Ok((b, bb)), Ok((c, cb))) => Ok((a + b + c + 34, ab + bb + cb + 34 * size_of::<(Annotation, DiagnosticMessage)>())),
                (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => Err(error),
            }
        }?;
        self.local(work, bytes, || {
            diagnostic.annotate(Annotation::primary(metadata.primary_span.clone()).message(message));
        }).await
    }
}
