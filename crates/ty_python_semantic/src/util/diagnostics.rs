use crate::{Db, Program, PythonVersionWithSource};
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, Severity, Span, SubDiagnostic},
    files::system_path_to_file,
};

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
        crate::PythonVersionSource::File(path, range) => {
            if let Ok(file) = system_path_to_file(db.upcast(), &**path) {
                let mut sub_diagnostic = SubDiagnostic::new(
                    Severity::Info,
                    format_args!("Python {version} was assumed when {action}"),
                );
                sub_diagnostic.annotate(
                    Annotation::primary(Span::from(file).with_optional_range(*range)).message(
                        format_args!("Python {version} assumed due to this configuration setting"),
                    ),
                );
                diagnostic.sub(sub_diagnostic);
            } else {
                diagnostic.info(format_args!(
                    "Python {version} was assumed when {action} because of your configuration file(s)",
                ));
            }
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
