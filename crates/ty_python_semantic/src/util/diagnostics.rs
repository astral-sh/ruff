use crate::{Db, Program, PythonVersionWithSource};
use ruff_db::diagnostic::{Annotation, Diagnostic, Severity, SubDiagnostic};
use std::fmt::Write;

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
                    Severity::Info,
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
                    Severity::Info,
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
        crate::PythonVersionSource::PythonVSCodeExtension => {
            diagnostic.info(format_args!(
                "Python {version} was assumed when {action} \
                because it's the version of the selected Python interpreter in the VS Code Python extension",
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
