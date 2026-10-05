//! Iterative searches borrow the prepared AST and retain only flat continuation frames.

#[cfg(test)]
mod tests;

use std::convert::Infallible;

use ruff_python_ast as ast;
use ruff_python_ast::helpers::{ExpressionSearchFrame, ExpressionSearchStep};
use smallvec::SmallVec;
use ty_mapping_probe_macros::shared_semantic_family;

pub(in crate::types::infer) struct ExpressionSearchCursor<'ast> {
    active: Option<ExpressionSearchFrame<'ast>>,
    parents: SmallVec<[ExpressionSearchFrame<'ast>; 4]>,
    #[cfg(all(test, feature = "experimental-analysis"))]
    lifetime: Option<observations::ScanLifetime>,
}

/// A prospective transition owns no source or storage. Admission can reject it without changing
/// the cursor retained by the class continuation.
#[derive(Clone, Copy)]
pub(super) enum ExpressionSearchPlan<'ast> {
    Visit {
        active: ExpressionSearchFrame<'ast>,
        expression: &'ast ast::Expr,
    },
    Enter {
        parent: ExpressionSearchFrame<'ast>,
        child: ExpressionSearchFrame<'ast>,
    },
    Progress(ExpressionSearchFrame<'ast>),
    Leave,
    Finished,
}

impl ExpressionSearchPlan<'_> {
    pub(super) fn enters_child(&self) -> bool {
        matches!(self, Self::Enter { .. })
    }
}

pub(in crate::types::infer) enum ExpressionSearchVisit<'ast> {
    Visit(&'ast ast::Expr),
    Progress,
}

impl<'ast> ExpressionSearchCursor<'ast> {
    pub(super) fn new(initial: ExpressionSearchFrame<'ast>) -> Self {
        Self {
            active: Some(initial),
            parents: SmallVec::new(),
            #[cfg(all(test, feature = "experimental-analysis"))]
            lifetime: None,
        }
    }

    pub(super) fn storage(&self) -> (usize, usize) {
        (self.parents.len(), self.parents.capacity())
    }

    pub(super) fn plan(&self) -> ExpressionSearchPlan<'ast> {
        let Some(mut active) = self.active else {
            return ExpressionSearchPlan::Finished;
        };
        match active.step() {
            ExpressionSearchStep::Visit(expression) => {
                ExpressionSearchPlan::Visit { active, expression }
            }
            ExpressionSearchStep::Enter(child) => ExpressionSearchPlan::Enter {
                parent: active,
                child,
            },
            ExpressionSearchStep::Progress => ExpressionSearchPlan::Progress(active),
            ExpressionSearchStep::Done => ExpressionSearchPlan::Leave,
        }
    }

    pub(super) fn commit(
        &mut self,
        plan: ExpressionSearchPlan<'ast>,
        additional: usize,
    ) -> Option<ExpressionSearchVisit<'ast>> {
        match plan {
            ExpressionSearchPlan::Visit { active, expression } => {
                self.active = Some(active);
                Some(ExpressionSearchVisit::Visit(expression))
            }
            ExpressionSearchPlan::Enter { parent, child } => {
                self.parents.reserve_exact(additional);
                self.parents.push(parent);
                self.active = Some(child);
                Some(ExpressionSearchVisit::Progress)
            }
            ExpressionSearchPlan::Progress(active) => {
                self.active = Some(active);
                Some(ExpressionSearchVisit::Progress)
            }
            ExpressionSearchPlan::Leave => {
                self.active = self.parents.pop();
                self.active.map(|_| ExpressionSearchVisit::Progress)
            }
            ExpressionSearchPlan::Finished => None,
        }
    }

    #[cfg(all(test, feature = "experimental-analysis"))]
    pub(super) fn observe_lifetime(&mut self) {
        self.lifetime = Some(observations::ScanLifetime::new());
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousExpressionSearchEffects)]
    pub(in crate::types::infer) trait ExpressionSearchEffects {
        type Error;

        #[operation(local)]
        async fn start<'ast>(&self, expressions: &'ast [ast::Expr]) -> Result<ExpressionSearchCursor<'ast>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next<'ast>(&self, cursor: &mut ExpressionSearchCursor<'ast>) -> Result<Option<ExpressionSearchVisit<'ast>>, Self::Error>;
        #[operation(local)]
        async fn is_string_literal(&self, expression: &ast::Expr) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn finish(&self, cursor: &mut ExpressionSearchCursor<'_>, found: bool) -> Result<bool, Self::Error>;
    }

    #[synchronous(contains_string_literal_sync)]
    #[capabilities(effects = ExpressionSearchEffects)]
    #[passive_values()]
    pub(in crate::types::infer) async fn contains_string_literal_with<E: ExpressionSearchEffects>(
        expressions: &[ast::Expr],
        effects: &E,
    ) -> Result<bool, E::Error> {
        let mut cursor = effects.start(expressions).await?;
        #[cursor_loop]
        while let Some(step) = effects.next(&mut cursor).await? {
            if let ExpressionSearchVisit::Visit(expression) = step
                && effects.is_string_literal(expression).await?
            {
                return effects.finish(&mut cursor, true).await;
            }
        }
        effects.finish(&mut cursor, false).await
    }
}

struct OrdinaryExpressionSearchEffects;

impl SynchronousExpressionSearchEffects for OrdinaryExpressionSearchEffects {
    type Error = Infallible;

    fn start<'ast>(
        &self,
        expressions: &'ast [ast::Expr],
    ) -> Result<ExpressionSearchCursor<'ast>, Self::Error> {
        Ok(ExpressionSearchCursor::new(
            ExpressionSearchFrame::expressions(expressions),
        ))
    }

    fn next<'ast>(
        &self,
        cursor: &mut ExpressionSearchCursor<'ast>,
    ) -> Result<Option<ExpressionSearchVisit<'ast>>, Self::Error> {
        Ok(cursor.commit(cursor.plan(), 0))
    }

    fn is_string_literal(&self, expression: &ast::Expr) -> Result<bool, Self::Error> {
        Ok(expression.is_string_literal_expr())
    }

    fn finish(
        &self,
        _cursor: &mut ExpressionSearchCursor<'_>,
        found: bool,
    ) -> Result<bool, Self::Error> {
        Ok(found)
    }
}

pub(in crate::types::infer) fn contains_string_literal(expressions: &[ast::Expr]) -> bool {
    let Ok(found) = contains_string_literal_sync(expressions, &OrdinaryExpressionSearchEffects);
    found
}

#[cfg(all(test, feature = "experimental-analysis"))]
pub(in crate::types::infer) mod observations {
    use std::cell::{Cell, RefCell};

    use ruff_text_size::TextRange;

    use crate::Db;

    #[derive(Clone, Copy, Debug)]
    pub(in crate::types::infer) struct GrowthObservation {
        pub ordinal: usize,
        pub len: usize,
        pub capacity: usize,
        pub remaining: Option<usize>,
    }

    #[derive(Clone, Debug, Default)]
    pub(in crate::types::infer) struct ScanProgress {
        pub live: usize,
        pub created: usize,
        pub retired: usize,
        pub before_growth: Vec<GrowthObservation>,
        pub after_growth: Vec<GrowthObservation>,
        pub visits: Vec<TextRange>,
    }

    thread_local! {
        static PROGRESS: RefCell<ScanProgress> = RefCell::default();
        static CANCEL_AFTER_GROWTH: Cell<Option<usize>> = const { Cell::new(None) };
    }

    pub(in crate::types::infer) fn reset(cancel_after_growth: Option<usize>) {
        PROGRESS.with_borrow_mut(|progress| {
            assert_eq!(progress.live, 0);
            *progress = ScanProgress::default();
        });
        CANCEL_AFTER_GROWTH.set(cancel_after_growth);
    }

    pub(in crate::types::infer) fn progress() -> ScanProgress {
        PROGRESS.with_borrow(Clone::clone)
    }

    pub(in crate::types::infer::builder) fn before_growth(db: &dyn Db, storage: (usize, usize)) {
        PROGRESS.with_borrow_mut(|progress| {
            let ordinal = progress.before_growth.len() + 1;
            progress.before_growth.push(GrowthObservation {
                ordinal,
                len: storage.0,
                capacity: storage.1,
                remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
            });
        });
    }

    pub(in crate::types::infer::builder) fn after_growth(db: &dyn Db, storage: (usize, usize)) {
        let ordinal = PROGRESS.with_borrow_mut(|progress| {
            let ordinal = progress.before_growth.len();
            progress.after_growth.push(GrowthObservation {
                ordinal,
                len: storage.0,
                capacity: storage.1,
                remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
            });
            ordinal
        });
        if CANCEL_AFTER_GROWTH.get() == Some(ordinal) {
            CANCEL_AFTER_GROWTH.set(None);
            db.cancellation_token().cancel();
        }
    }

    pub(in crate::types::infer::builder) fn visit(range: TextRange) {
        PROGRESS.with_borrow_mut(|progress| progress.visits.push(range));
    }

    pub(super) struct ScanLifetime;

    impl ScanLifetime {
        pub(super) fn new() -> Self {
            PROGRESS.with_borrow_mut(|progress| {
                progress.created += 1;
                progress.live += 1;
            });
            Self
        }
    }

    impl Drop for ScanLifetime {
        fn drop(&mut self) {
            PROGRESS.with_borrow_mut(|progress| {
                progress.retired += 1;
                progress.live -= 1;
            });
        }
    }
}
