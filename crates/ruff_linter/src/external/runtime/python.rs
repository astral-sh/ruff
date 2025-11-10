#[cfg(not(Py_GIL_DISABLED))]
compile_error!(
    "The external runtime now assumes a free-threaded CPython build. \
     Rebuild PyO3 with `UNSAFE_PYO3_BUILD_FREE_THREADED=1` so that `Py_GIL_DISABLED` is set."
);
thread_local! {
    static PY_SESSION_DEPTH: Cell<usize> = const { Cell::new(0) };
}

use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::fmt;
use std::hash::Hasher;
use std::sync::{Arc, Mutex, OnceLock};

use crate::checkers::ast::Checker;
use crate::external::RuleLocator;
use crate::external::ast::python::{
    ModuleTypes, ProjectionTypes, expr_to_python, load_module_types, stmt_to_python,
};
use crate::external::ast::registry::ExternalLintRegistry;
use crate::external::ast::target::{AstTarget, ExprKind, StmtKind};
use crate::external::error::ExternalLinterError;
use crate::warn_user;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule};
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};

thread_local! {
    static RUNTIME_CACHE: RefCell<FxHashMap<u64, RegistryRuntime>> =
        RefCell::new(FxHashMap::default());
}

static VERIFIED_REGISTRIES: OnceLock<Mutex<FxHashSet<u64>>> = OnceLock::new();

#[derive(Debug)]
pub(crate) struct ExternalRuleHandle {
    check_stmt: Option<Py<PyAny>>,
    check_expr: Option<Py<PyAny>>,
}

#[derive(Clone, Debug)]
pub(crate) struct RuntimeEnvironment {
    module_types: Arc<ModuleTypes>,
}

impl RuntimeEnvironment {
    fn new(module_types: ModuleTypes) -> Self {
        Self {
            module_types: Arc::new(module_types),
        }
    }

    fn module_types(&self) -> Arc<ModuleTypes> {
        Arc::clone(&self.module_types)
    }
}

pub(crate) type CompiledCodeMap = FxHashMap<RuleLocator, ExternalRuleHandle>;

#[derive(Clone)]
pub(crate) struct ExternalLintRuntime {
    registry: Arc<ExternalLintRegistry>,
    runtime_cache: RuntimeCache,
}

impl ExternalLintRuntime {
    pub(crate) fn new(registry: ExternalLintRegistry) -> Self {
        let mut hasher = CacheKeyHasher::new();
        registry.cache_key(&mut hasher);
        let pool_id = hasher.finish();
        ensure_registry_verified(pool_id, &registry);

        let registry = Arc::new(registry);
        Self {
            runtime_cache: RuntimeCache::new(pool_id),
            registry,
        }
    }

    pub(crate) fn registry(&self) -> &ExternalLintRegistry {
        self.registry.as_ref()
    }

    pub(crate) fn run_on_stmt(&self, checker: &Checker<'_>, stmt: &Stmt) {
        self.run_on_stmt_with_kind(checker, StmtKind::from(stmt), stmt);
    }

    pub(crate) fn run_on_expr(&self, checker: &Checker<'_>, expr: &Expr) {
        self.run_on_expr_with_kind(checker, ExprKind::from(expr), expr);
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn run_in_session<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        with_attached_python(|_| f())
    }

    fn run_on_stmt_with_kind(&self, checker: &Checker<'_>, kind: StmtKind, stmt: &Stmt) {
        let locators: Vec<_> = self.registry.rules_for_stmt(kind).collect();
        self.dispatch_rules(
            checker,
            stmt,
            &locators,
            |_| true,
            |handle| handle.check_stmt.as_ref(),
            stmt_to_python,
        );
    }

    fn run_on_expr_with_kind(&self, checker: &Checker<'_>, kind: ExprKind, expr: &Expr) {
        let locators: Vec<_> = self.registry.rules_for_expr(kind).collect();
        let mut call_callee_cache = CachedCallee::default();
        self.dispatch_rules(
            checker,
            expr,
            &locators,
            |rule| rule_applicable_to_expr(rule, kind, expr, &mut call_callee_cache),
            |handle| handle.check_expr.as_ref(),
            expr_to_python,
        );
    }

    fn dispatch_rules<'node, Node>(
        &self,
        checker: &Checker<'_>,
        node: &'node Node,
        locators: &[RuleLocator],
        mut should_run: impl FnMut(&crate::external::ast::rule::ExternalAstRule) -> bool,
        get_callback: impl Fn(&ExternalRuleHandle) -> Option<&Py<PyAny>>,
        convert: impl Fn(
            Python<'_>,
            &crate::Locator<'_>,
            &'node Node,
            &ProjectionTypes,
        ) -> PyResult<PyObject>,
    ) where
        Node: Ranged + 'node,
    {
        if locators.is_empty() {
            return;
        }

        with_attached_python(|py| {
            self.runtime_cache
                .with_runtime(self.registry.as_ref(), |environment, compiled| {
                    let module_types = environment.module_types();
                    let source = checker.locator();
                    for &rule_locator in locators {
                        let (_linter, rule) = self.registry.expect_entry(rule_locator);
                        if !should_run(rule) {
                            continue;
                        }
                        if let Some(handle) = compiled.get(&rule_locator) {
                            if let Some(callback) = get_callback(handle) {
                                let result = (|| -> PyResult<()> {
                                    let py_node =
                                        convert(py, source, node, &module_types.projection)?;
                                    let context = build_context(
                                        py,
                                        module_types.as_ref(),
                                        rule,
                                        node.range(),
                                    )?;
                                    let outcome =
                                        callback.call1(py, (py_node, context.context(py)));
                                    context.flush(py, checker);
                                    outcome.map(|_| ())
                                })();
                                if let Err(err) = result {
                                    self.report_python_error(py, rule_locator, &err);
                                }
                            }
                        }
                    }
                });
        });
    }

    fn report_python_error(&self, py: Python<'_>, locator: RuleLocator, err: &PyErr) {
        let (linter, rule) = self.registry.expect_entry(locator);
        warn_user!(
            "Error while executing external rule `{}` in linter `{}`: {err}",
            rule.code.as_str(),
            linter.id
        );
        err.print(py);
    }
}

impl fmt::Debug for ExternalLintRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("ExternalLintRuntime");
        debug.field("registry", &self.registry);
        debug.field("runtime_cache", &self.runtime_cache);
        debug.finish()
    }
}

struct RegistryRuntime {
    environment: RuntimeEnvironment,
    compiled: CompiledCodeMap,
}

impl fmt::Debug for RegistryRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegistryRuntime")
            .field("environment", &"RuntimeEnvironment")
            .field("compiled_rules", &self.compiled.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug)]
struct RuntimeCache {
    id: u64,
}

impl RuntimeCache {
    fn new(id: u64) -> Self {
        Self { id }
    }

    fn with_runtime<F, R>(self, registry: &ExternalLintRegistry, f: F) -> R
    where
        F: FnOnce(&RuntimeEnvironment, &CompiledCodeMap) -> R,
    {
        RUNTIME_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let entry = cache.entry(self.id).or_insert_with(|| {
                create_registry_runtime(registry).unwrap_or_else(|error| {
                    panic!("failed to initialize external linter runtime: {error}")
                })
            });
            f(&entry.environment, &entry.compiled)
        })
    }
}

fn create_registry_runtime(
    registry: &ExternalLintRegistry,
) -> Result<RegistryRuntime, ExternalLinterError> {
    let (module_types, compiled) = compile_scripts(registry)?;
    let environment = RuntimeEnvironment::new(module_types);
    Ok(RegistryRuntime {
        environment,
        compiled,
    })
}

fn ensure_python_initialized() {
    static PYTHON_INIT: OnceLock<()> = OnceLock::new();
    PYTHON_INIT.get_or_init(|| {
        pyo3::prepare_freethreaded_python();
    });
}

#[allow(unsafe_code)]
fn with_attached_python<F, R>(f: F) -> R
where
    F: for<'py> FnOnce(Python<'py>) -> R,
{
    struct DepthGuard<'a>(&'a Cell<usize>);
    impl Drop for DepthGuard<'_> {
        fn drop(&mut self) {
            let current = self.0.get();
            debug_assert!(current > 0);
            self.0.set(current - 1);
        }
    }

    ensure_python_initialized();

    PY_SESSION_DEPTH.with(|depth| {
        if depth.get() > 0 {
            unsafe { f(Python::assume_gil_acquired()) }
        } else {
            Python::with_gil(|py| {
                depth.set(1);
                let _guard = DepthGuard(depth);
                f(py)
            })
        }
    })
}

fn compile_scripts(
    registry: &ExternalLintRegistry,
) -> Result<(ModuleTypes, CompiledCodeMap), ExternalLinterError> {
    with_attached_python(|py| -> Result<_, ExternalLinterError> {
        let module_types =
            load_module_types(py).map_err(|err| ExternalLinterError::ScriptCompile {
                message: format!("failed to initialize external runtime module: {err}"),
            })?;

        let mut compiled = FxHashMap::default();
        let mut errors = Vec::new();

        for locator in registry.iter_enabled_rule_locators() {
            let (linter, rule) = registry.expect_entry(locator);
            // `PyModule::from_code` uses the module name to populate `sys.modules`. Ensure that
            // the name is unique per script to avoid cross-rule (and cross-test) contamination
            // when scripts are compiled in parallel.
            let script_hash = {
                let mut hasher = CacheKeyHasher::new();
                let path_str = rule.script.path().to_string_lossy();
                hasher.write_usize(path_str.len());
                hasher.write(path_str.as_bytes());
                let contents_str = rule.script.body();
                hasher.write_usize(contents_str.len());
                hasher.write(contents_str.as_bytes());
                hasher.finish()
            };
            let module_name = format!(
                "ruff_external_{}_{}_{script_hash:016x}",
                linter.id,
                rule.code.as_str()
            );
            let Ok(code_cstr) = CString::new(rule.script.body()) else {
                errors.push(ExternalLinterError::format_script_compile_message(
                    linter.id.as_ref(),
                    rule.code.as_str(),
                    Some(rule.script.path().to_path_buf()),
                    "script body contains an interior NUL byte",
                ));
                continue;
            };
            let file_name_owned = rule.script.path().to_string_lossy();
            let Ok(file_cstr) = CString::new(file_name_owned.as_ref()) else {
                errors.push(ExternalLinterError::format_script_compile_message(
                    linter.id.as_ref(),
                    rule.code.as_str(),
                    Some(rule.script.path().to_path_buf()),
                    "script path contains an interior NUL byte",
                ));
                continue;
            };
            let Ok(module_cstr) = CString::new(module_name) else {
                errors.push(ExternalLinterError::format_script_compile_message(
                    linter.id.as_ref(),
                    rule.code.as_str(),
                    Some(rule.script.path().to_path_buf()),
                    "module name contains an interior NUL byte",
                ));
                continue;
            };
            match PyModule::from_code(
                py,
                code_cstr.as_c_str(),
                file_cstr.as_c_str(),
                module_cstr.as_c_str(),
            ) {
                Ok(module) => match build_rule_handle(&module, linter.id.as_ref(), rule) {
                    Ok(handle) => {
                        compiled.insert(locator, handle);
                    }
                    Err(err) => errors.push(err),
                },
                Err(err) => errors.push(ExternalLinterError::format_script_compile_message(
                    linter.id.as_ref(),
                    rule.name.as_ref(),
                    Some(rule.script.path().to_path_buf()),
                    err.to_string(),
                )),
            }
        }

        if !errors.is_empty() {
            return Err(ExternalLinterError::ScriptCompile {
                message: errors.join("\n"),
            });
        }

        Ok((module_types, compiled))
    })
}

fn build_rule_handle(
    module: &Bound<'_, PyModule>,
    linter: &str,
    rule: &crate::external::ast::rule::ExternalAstRule,
) -> Result<ExternalRuleHandle, String> {
    let needs_stmt = rule
        .targets
        .iter()
        .any(|target| matches!(target, AstTarget::Stmt(_)));
    let needs_expr = rule
        .targets
        .iter()
        .any(|target| matches!(target, AstTarget::Expr(_)));

    let check_stmt = lookup_callable(module, "check_stmt");
    let check_expr = lookup_callable(module, "check_expr");

    if needs_stmt && check_stmt.is_none() {
        return Err(ExternalLinterError::MissingHandler {
            linter: linter.to_string(),
            rule: rule.name.clone(),
            handler: "check_stmt".to_string(),
            target: "stmt".to_string(),
        }
        .to_string());
    }

    if needs_expr && check_expr.is_none() {
        return Err(ExternalLinterError::MissingHandler {
            linter: linter.to_string(),
            rule: rule.name.clone(),
            handler: "check_expr".to_string(),
            target: "expr".to_string(),
        }
        .to_string());
    }

    Ok(ExternalRuleHandle {
        check_stmt,
        check_expr,
    })
}

fn lookup_callable(module: &Bound<'_, PyModule>, name: &str) -> Option<Py<PyAny>> {
    match module.getattr(name) {
        Ok(value) if value.is_callable() => Some(value.into_any().unbind()),
        _ => None,
    }
}

struct RuntimeContext {
    context: PyObject,
    reporter: Py<PyReporter>,
}

impl RuntimeContext {
    fn context(&self, py: Python<'_>) -> PyObject {
        self.context.clone_ref(py)
    }

    fn flush(&self, py: Python<'_>, checker: &Checker<'_>) {
        let reporter = self.reporter.bind(py);
        reporter.borrow().drain_into(checker);
    }
}

fn build_context(
    py: Python<'_>,
    module_types: &ModuleTypes,
    rule: &crate::external::ast::rule::ExternalAstRule,
    range: TextRange,
) -> PyResult<RuntimeContext> {
    let reporter = PyReporter::new(py, rule, range)?;
    let context = module_types
        .context
        .bind(py)
        .call1((
            rule.code.as_str(),
            rule.name.as_str(),
            reporter.clone_ref(py),
        ))?
        .into();

    Ok(RuntimeContext { context, reporter })
}

fn rule_applicable_to_expr(
    rule: &crate::external::ast::rule::ExternalAstRule,
    kind: ExprKind,
    expr: &Expr,
    call_callee_cache: &mut CachedCallee,
) -> bool {
    match rule.call_callee() {
        Some(matcher) => {
            if kind != ExprKind::Call {
                return false;
            }

            let callee = call_callee_cache.resolve(expr);
            match callee {
                Some(callee) => matcher.regex().is_match(callee),
                None => false,
            }
        }
        None => true,
    }
}

fn extract_call_callee(expr: &Expr) -> Option<String> {
    let call = expr.as_call_expr()?;
    UnqualifiedName::from_expr(call.func.as_ref()).map(|name| name.to_string())
}

#[derive(Default)]
struct CachedCallee {
    cached: CachedValue,
}

#[derive(Default)]
enum CachedValue {
    #[default]
    Unknown,
    Known(Option<String>),
}

impl CachedCallee {
    fn resolve<'expr>(&'expr mut self, expr: &Expr) -> Option<&'expr str> {
        if matches!(self.cached, CachedValue::Unknown) {
            self.cached = CachedValue::Known(extract_call_callee(expr));
        }

        match self.cached {
            CachedValue::Known(Some(ref value)) => Some(value.as_str()),
            _ => None,
        }
    }
}

fn ensure_registry_verified(id: u64, registry: &ExternalLintRegistry) {
    let cache = VERIFIED_REGISTRIES.get_or_init(|| Mutex::new(FxHashSet::default()));
    let mut cache = cache.lock().expect("verification cache poisoned");
    if cache.contains(&id) {
        return;
    }
    verify_registry_scripts(registry)
        .unwrap_or_else(|error| panic!("failed to compile external scripts: {error}"));
    cache.insert(id);
}

pub fn verify_registry_scripts(registry: &ExternalLintRegistry) -> Result<(), ExternalLinterError> {
    compile_scripts(registry).map(|_| ())
}

mod reporter {
    #![allow(unsafe_op_in_unsafe_fn)]

    use crate::checkers::ast::Checker;
    use crate::rules::ruff::rules::external_ast::ExternalLinter as ExternalLinterViolation;
    use pyo3::prelude::*;
    use ruff_db::diagnostic::SecondaryCode;
    use ruff_text_size::{TextRange, TextSize};
    use std::cell::RefCell;

    #[pyclass(module = "ruff_external", unsendable)]
    pub(crate) struct PyReporter {
        diagnostics: RefCell<Vec<PendingDiagnostic>>,
        rule_code: String,
        rule_name: String,
        default_span: (u32, u32),
    }

    #[derive(Debug)]
    struct PendingDiagnostic {
        message: String,
        span: Option<(u32, u32)>,
    }

    impl PyReporter {
        pub(crate) fn new(
            py: Python<'_>,
            rule: &crate::external::ast::rule::ExternalAstRule,
            range: TextRange,
        ) -> PyResult<Py<PyReporter>> {
            Py::new(
                py,
                PyReporter {
                    diagnostics: RefCell::new(Vec::new()),
                    rule_code: rule.code.as_str().to_string(),
                    rule_name: rule.name.clone(),
                    default_span: (range.start().to_u32(), range.end().to_u32()),
                },
            )
        }

        pub(crate) fn drain_into(&self, checker: &Checker<'_>) {
            let pending = std::mem::take(&mut *self.diagnostics.borrow_mut());
            for diagnostic in pending {
                let range = self.resolve_span(diagnostic.span);
                let mut emitted = checker.report_diagnostic(
                    ExternalLinterViolation {
                        rule_name: self.rule_name.clone(),
                        message: diagnostic.message,
                    },
                    range,
                );
                emitted.set_secondary_code(SecondaryCode::new(self.rule_code.clone()));
            }
        }

        fn resolve_span(&self, span: Option<(u32, u32)>) -> TextRange {
            let (start, end) = match span {
                Some((start, end)) if end >= start => (start, end),
                _ => self.default_span,
            };
            TextRange::new(TextSize::new(start), TextSize::new(end))
        }
    }

    #[pymethods]
    impl PyReporter {
        #[pyo3(signature = (message, span=None))]
        fn __call__(&self, message: &str, span: Option<(u32, u32)>) {
            self.diagnostics.borrow_mut().push(PendingDiagnostic {
                message: message.to_string(),
                span,
            });
        }
    }
}

use reporter::PyReporter;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::external::ast::rule::{
        ExternalAstLinter, ExternalAstRule, ExternalRuleCode, ExternalRuleScript,
    };
    use crate::external::ast::target::{AstTarget, StmtKind};

    fn basic_rule(code: &str, script: ExternalRuleScript) -> ExternalAstRule {
        ExternalAstRule::new(
            ExternalRuleCode::new(code).unwrap(),
            "ExampleRule",
            None::<&str>,
            vec![AstTarget::Stmt(StmtKind::FunctionDef)],
            script,
            None,
        )
    }

    #[test]
    fn interpreter_runs_basic_code() {
        let runtime = ExternalLintRuntime::new(ExternalLintRegistry::new());
        runtime
            .runtime_cache
            .with_runtime(runtime.registry(), |_, _| {
                with_attached_python(|py| {
                    let code = CString::new("40 + 2").unwrap();
                    let result: i32 = py
                        .eval(code.as_c_str(), None, None)
                        .unwrap()
                        .extract()
                        .unwrap();
                    assert_eq!(result, 42);
                });
            });
    }

    #[test]
    fn compile_errors_surface_during_validation() {
        let mut registry = ExternalLintRegistry::new();
        let rule = basic_rule(
            "EXT001",
            ExternalRuleScript::file(PathBuf::from("broken.py"), "def broken(:\n"),
        );
        let linter = ExternalAstLinter::new("broken", "Broken", None::<&str>, true, vec![rule]);
        registry.insert_linter(linter).unwrap();

        let other_rule = basic_rule(
            "EXT002",
            ExternalRuleScript::file(PathBuf::from("other.py"), "def also_broken(:\n"),
        );
        let other_linter =
            ExternalAstLinter::new("other", "Other", None::<&str>, true, vec![other_rule]);
        registry.insert_linter(other_linter).unwrap();

        let err = verify_registry_scripts(&registry).expect_err("expected compile failure");
        match err {
            ExternalLinterError::ScriptCompile { message } => {
                assert!(message.contains("broken"), "message: {message}");
                assert!(message.contains("other"), "message: {message}");
                assert!(
                    message.lines().count() >= 2,
                    "expected multiple lines: {message}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
