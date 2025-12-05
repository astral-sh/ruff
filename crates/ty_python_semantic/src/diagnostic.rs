use crate::{
    Db, Program, PythonVersionWithSource,
    lint::lint_documentation_url,
    types::{Type, TypeCheckDiagnostics, list_members::all_members},
};
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, DiagnosticId, SubDiagnostic, SubDiagnosticSeverity},
    files::File,
};
use std::cell::RefCell;
use std::fmt::Write;

pub(crate) fn did_you_mean_for_unresolved_member<'db>(
    db: &'db dyn Db,
    obj: Type<'db>,
    unresolved_member: &str,
) -> Option<String> {
    did_you_mean(
        all_members(db, obj)
            .iter()
            .map(|member| member.name.as_str()),
        unresolved_member,
    )
}

/// Suggest a name from `existing_names` that is similar to `wrong_name`.
/// The suggestion algorithm is inspired by [rustc](https://doc.rust-lang.org/beta/nightly-rustc/src/rustc_span/edit_distance.rs.html).
pub(crate) fn did_you_mean<S: AsRef<str>, T: AsRef<str>>(
    existing_names: impl Iterator<Item = S>,
    wrong_name: T,
) -> Option<String> {
    /// Edit distance with a favor rule that the score is halved if one string starts or ends with the other
    fn edit_distance_with_substring_preferential_rule(candidate: &str, wrong_name: &str) -> usize {
        let candidate_len = candidate.chars().count();
        let wrong_name_len = wrong_name.chars().count();

        let (shorter, shorter_len) = if candidate_len < wrong_name_len {
            (candidate, candidate_len)
        } else {
            (wrong_name, wrong_name_len)
        };
        let (longer, longer_len) = if candidate_len < wrong_name_len {
            (wrong_name, wrong_name_len)
        } else {
            (candidate, candidate_len)
        };

        // Use OSA (restricted Damerau-Levenshtein) distance, as rustc does.
        let osa_distance = strsim::osa_distance(shorter, longer);

        let not_so_different_in_length = (shorter_len * 2) > longer_len;
        // `longer` starts or ends with `shorter` and not so different in length, give it a bonus by halving the distance.
        if osa_distance > 1
            && not_so_different_in_length
            && (longer.starts_with(shorter) || longer.ends_with(shorter))
        {
            osa_distance.div_ceil(2)
        } else {
            osa_distance
        }
    }

    /// Heuristic to filter out bad matches
    fn distance_filter(dist: usize, wrong_name_len: usize, best_score: &mut Option<usize>) -> bool {
        // Don't consider any candidates that are inferior to the best candidate we have had so far.
        if let Some(best) = best_score {
            if dist > *best {
                return false;
            }
        }
        // Differences up to 1/3 of the total string length are considered candidates (If 3 characters or less, the distance limit is rounded up to 1).
        let match_condition = dist <= wrong_name_len.max(3) / 3;
        if match_condition {
            *best_score = Some(dist);
        }

        match_condition
    }

    let wrong_name_len = wrong_name.as_ref().chars().count();
    if wrong_name_len < 3 {
        return None;
    }

    let mut best_score: Option<usize> = None;

    existing_names
        .filter(|ref id| id.as_ref().len() >= 2)
        .map(|ref id| {
            (
                id.as_ref().to_string(),
                edit_distance_with_substring_preferential_rule(
                    &id.as_ref().to_lowercase(),
                    &wrong_name.as_ref().to_lowercase(),
                ),
            )
        })
        .filter(|(_, dist)| distance_filter(*dist, wrong_name_len, &mut best_score))
        // At each iteration, the best candidate is retained, so the last candidate has the smallest score.
        .last()
        .map(|(id, _)| id)
}

/// Add a subdiagnostic to `diagnostic` that explains why a certain Python version was inferred.
///
/// ty can infer the Python version from various sources, such as command-line arguments,
/// configuration files, or defaults.
pub fn add_inferred_python_version_hint_to_diagnostic(
    db: &dyn Db,
    diagnostic: &mut Diagnostic,
    action: &str,
) {
    let program = Program::get(db);
    let PythonVersionWithSource { version, source } = program.python_version_with_source(db);

    match source {
        crate::PythonVersionSource::Cli => {
            diagnostic.info(format_args!(
                "Python {version} was assumed when {action} because it was specified on the command line",
            ));
        }
        crate::PythonVersionSource::ConfigFile(source) => {
            if let Some(span) = source.span(db) {
                let mut sub_diagnostic = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format_args!("Python {version} was assumed when {action}"),
                );
                sub_diagnostic.annotate(Annotation::primary(span).message(format_args!(
                    "Python {version} assumed due to this configuration setting"
                )));
                diagnostic.sub(sub_diagnostic);
            } else {
                diagnostic.info(format_args!(
                    "Python {version} was assumed when {action} because of your configuration file(s)",
                ));
            }
        }
        crate::PythonVersionSource::PyvenvCfgFile(source) => {
            if let Some(span) = source.span(db) {
                let mut sub_diagnostic = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format_args!(
                        "Python {version} was assumed when {action} because of your virtual environment"
                    ),
                );
                sub_diagnostic.annotate(
                    Annotation::primary(span)
                        .message("Python version inferred from virtual environment metadata file"),
                );
                // TODO: it would also be nice to tell them how we resolved their virtual environment...
                diagnostic.sub(sub_diagnostic);
            } else {
                diagnostic.info(format_args!(
                    "Python {version} was assumed when {action} because \
                    your virtual environment's pyvenv.cfg file indicated \
                    it was the Python version being used",
                ));
            }
            diagnostic.info(
                "No Python version was specified on the command line \
                or in a configuration file",
            );
        }
        crate::PythonVersionSource::Editor => {
            diagnostic.info(format_args!(
                "Python {version} was assumed when {action} \
                because it's the version of the selected Python interpreter in your editor",
            ));
        }
        crate::PythonVersionSource::InstallationDirectoryLayout {
            site_packages_parent_dir,
        } => {
            // TODO: it would also be nice to tell them how we resolved this Python installation...
            diagnostic.info(format_args!(
                "Python {version} was assumed when {action} \
                because of the layout of your Python installation"
            ));
            diagnostic.info(format_args!(
                "The primary `site-packages` directory of your installation was found \
                at `lib/{site_packages_parent_dir}/site-packages/`"
            ));
            diagnostic.info(
                "No Python version was specified on the command line \
                or in a configuration file",
            );
        }
        crate::PythonVersionSource::Default => {
            diagnostic.info(format_args!(
                "Python {version} was assumed when {action} \
                because it is the newest Python version supported by ty, \
                and neither a command-line argument nor a configuration setting was provided",
            ));
        }
    }
}

/// Format a list of elements as a human-readable enumeration.
///
/// Encloses every element in backticks (`1`, `2` and `3`).
pub(crate) fn format_enumeration<I, IT, D>(elements: I) -> String
where
    I: IntoIterator<IntoIter = IT>,
    IT: ExactSizeIterator<Item = D> + DoubleEndedIterator,
    D: std::fmt::Display,
{
    let mut elements = elements.into_iter();
    debug_assert!(elements.len() >= 2);

    let final_element = elements.next_back().unwrap();
    let penultimate_element = elements.next_back().unwrap();

    let mut buffer = String::new();
    for element in elements {
        write!(&mut buffer, "`{element}`, ").ok();
    }
    write!(&mut buffer, "`{penultimate_element}` and `{final_element}`").ok();

    buffer
}

/// An abstraction for mutating a diagnostic.
///
/// Callers likely should use `LintDiagnosticGuard` via
/// `InferContext::report_lint` instead. This guard is only intended for use
/// with non-lint diagnostics or non-type checking diagnostics. It is fundamentally lower level and easier to
/// get things wrong by using it.
///
/// Unlike `LintDiagnosticGuard`, this API does not guarantee that the
/// constructed `Diagnostic` not only has a primary annotation, but its
/// associated file is equivalent to the file being type checked. As a result,
/// if either is violated, then the `Drop` impl on `DiagnosticGuard` will
/// panic.
pub(super) struct DiagnosticGuard<'sink> {
    /// The file of the primary span (to which file does this diagnostic belong).
    file: File,

    /// The target where to emit the diagnostic to.
    ///
    /// We use a [`RefCell`] here over a `&mut TypeCheckDiagnostics` to ensure the fact that
    /// `InferContext` (and other contexts with diagnostics) use a [`RefCell`] internally
    /// remains abstracted away. Specifically, we want to ensure that calling `report_lint` on
    /// `InferContext` twice doesn't result in a panic:
    ///
    /// ```ignore
    /// let diag1 = context.report_lint(...);
    ///
    /// // would panic if using a `&mut TypeCheckDiagnostics`
    /// // because of a second mutable borrow.
    /// let diag2 = context.report_lint(...);
    /// ```
    sink: &'sink RefCell<TypeCheckDiagnostics>,

    /// The diagnostic that we want to report.
    ///
    /// This is always `Some` until the `Drop` impl.
    diag: Option<Diagnostic>,
}

impl<'sink> DiagnosticGuard<'sink> {
    pub(crate) fn new(
        file: File,
        sink: &'sink std::cell::RefCell<TypeCheckDiagnostics>,
        diag: Diagnostic,
    ) -> Self {
        Self {
            file,
            sink,
            diag: Some(diag),
        }
    }
}

impl std::ops::Deref for DiagnosticGuard<'_> {
    type Target = Diagnostic;

    fn deref(&self) -> &Diagnostic {
        // OK because `self.diag` is only `None` within `Drop`.
        self.diag.as_ref().unwrap()
    }
}

/// Return a mutable borrow of the diagnostic in this guard.
///
/// Callers may mutate the diagnostic to add new sub-diagnostics
/// or annotations.
///
/// The diagnostic is added to the typing context, if appropriate,
/// when this guard is dropped.
impl std::ops::DerefMut for DiagnosticGuard<'_> {
    fn deref_mut(&mut self) -> &mut Diagnostic {
        // OK because `self.diag` is only `None` within `Drop`.
        self.diag.as_mut().unwrap()
    }
}

/// Finishes use of this guard.
///
/// This will add the diagnostic to the typing context if appropriate.
///
/// # Panics
///
/// This panics when the underlying diagnostic lacks a primary
/// annotation, or if it has one and its file doesn't match the file
/// being type checked.
impl Drop for DiagnosticGuard<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            // Don't submit diagnostics when panicking because they might be incomplete.
            return;
        }

        // OK because the only way `self.diag` is `None`
        // is via this impl, which can only run at most
        // once.
        let mut diag = self.diag.take().unwrap();

        let Some(ann) = diag.primary_annotation() else {
            panic!(
                "All diagnostics reported by `InferContext` must have a \
                 primary annotation, but diagnostic {id} does not",
                id = diag.id(),
            );
        };

        let expected_file = self.file;
        let got_file = ann.get_span().expect_ty_file();
        assert_eq!(
            expected_file,
            got_file,
            "All diagnostics reported by `InferContext` must have a \
             primary annotation whose file matches the file of the \
             current typing context, but diagnostic {id} has file \
             {got_file:?} and we expected {expected_file:?}",
            id = diag.id(),
        );

        if let DiagnosticId::Lint(lint_name) = diag.id()
            && diag.documentation_url().is_none()
        {
            diag.set_documentation_url(Some(lint_documentation_url(lint_name)));
        }

        self.sink.borrow_mut().push(diag);
    }
}
