//! Canonical scope-map queries select retained structural data without restarting inference.

use std::sync::Arc;

use salsa::execution_probe::{
    BorrowOrCopy, CallableRouteProvider, NativeValueOperation, NativeValueQuote, RunError,
    RunResult, TaskEndpoint,
};
use salsa::plumbing::function::Configuration;
use ty_python_core::scope::ScopeId;
use ty_python_core::{PlaceTable, UseDefMap};

use crate::analysis::AnalysisSession;

use super::native_values;

macro_rules! scope_map_provider {
    ($provider:ident, $table:ty, $select:ident) => {
        pub(super) struct $provider<'run, 'session, 'db> {
            pub(super) session: &'run AnalysisSession<'session, 'db>,
        }

        impl<'run, 'session, 'db: 'run, C> CallableRouteProvider<'run, 'db, C>
            for $provider<'run, 'session, 'db>
        where
            C: for<'a> Configuration<
                    DbView = dyn ty_python_core::Db,
                    Input<'a> = ScopeId<'a>,
                > + Configuration<Output<'db> = Arc<$table>>,
        {
            async fn native_value<'call>(
                &'call self,
                endpoint: TaskEndpoint<'run, 'db>,
                _db: &'db dyn ty_python_core::Db,
                operation: NativeValueOperation<'call, 'db, C>,
            ) -> RunResult<NativeValueQuote>
            where
                'run: 'call,
            {
                native_values::quote(endpoint, operation).await
            }

            async fn body<'call>(
                &'call self,
                endpoint: TaskEndpoint<'run, 'db>,
                _db: &'db dyn ty_python_core::Db,
                scope: ScopeId<'db>,
            ) -> RunResult<Arc<$table>>
            where
                'run: 'call,
            {
                let fields = scope.read_fields(endpoint.field_request_context());
                let file = endpoint.read_field(fields.program_file(), &BorrowOrCopy).await;
                let index = self.session.read_semantic_index(&endpoint, file).await?;
                let file_scope = endpoint.read_field(fields.file_scope_id(), &BorrowOrCopy).await;
                Ok(endpoint.local_call(|| {
                    // The prepared index retains this Arc through candidate cleanup. Selection,
                    // cloning and dropping the candidate therefore do not traverse the table.
                    endpoint.admit_work(3)?;
                    endpoint.check_completion()?;
                    Ok(index.$select(file_scope))
                }).await)
            }

            async fn initial<'call>(
                &'call self,
                endpoint: TaskEndpoint<'run, 'db>,
                _db: &'db dyn ty_python_core::Db,
                _id: salsa::Id,
                _scope: ScopeId<'db>,
            ) -> RunResult<Arc<$table>>
            where
                'run: 'call,
            {
                Ok(endpoint.local_call(|| Err(RunError::Contract(
                    "scope-map query unexpectedly formed a cycle",
                ))).await)
            }

            async fn recover<'call>(
                &'call self,
                endpoint: TaskEndpoint<'run, 'db>,
                _db: &'db dyn ty_python_core::Db,
                _cycle: &'call salsa::Cycle<'call>,
                _last: &'call Arc<$table>,
                value: Arc<$table>,
                _scope: ScopeId<'db>,
            ) -> RunResult<Arc<$table>>
            where
                'run: 'call,
            {
                let result = endpoint.local_call(|| Err(RunError::Contract(
                    "scope-map query unexpectedly required cycle recovery",
                ))).await;
                drop(value);
                Ok(result)
            }
        }
    };
}

scope_map_provider!(PlaceTableProvider, PlaceTable, place_table_arc);
scope_map_provider!(UseDefMapProvider, UseDefMap<'db>, use_def_map_arc);
