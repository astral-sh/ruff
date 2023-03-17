use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of legacy `np.random` function calls.
///
/// ## Why is this bad?
/// According to the NumPy documentation's [Legacy Random Generation]:
///
/// > The `RandomState` provides access to legacy generators... This class
/// > should only be used if it is essential to have randoms that are
/// > identical to what would have been produced by previous versions of
/// > NumPy.
///
/// The members exposed directly on the `random` module are convenience
/// functions that alias to methods on a global singleton `RandomState`
/// instance. NumPy recommends using a dedicated `Generator` instance
/// rather than the random variate generation methods exposed directly on
/// the `random` module, as the new `Generator` is both faster and has
/// better statistical properties.
///
/// See the documentation on [Random Sampling] and [NEP 19] for further
/// details.
///
/// ## Examples
/// ```python
/// import numpy as np
///
/// np.random.seed(1337)
/// np.random.normal()
/// ```
///
/// Use instead:
/// ```python
/// rng = np.random.default_rng(1337)
/// rng.normal()
/// ```
///
/// [Legacy Random Generation]: https://numpy.org/doc/stable/reference/random/legacy.html#legacy
/// [Random Sampling]: https://numpy.org/doc/stable/reference/random/index.html#random-quick-start
/// [NEP 19]: https://numpy.org/neps/nep-0019-rng-policy.html
#[violation]
pub struct NumpyLegacyRandom {
    pub method_name: String,
}

impl Violation for NumpyLegacyRandom {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NumpyLegacyRandom { method_name } = self;
        format!("Replace legacy `np.random.{method_name}` call with `np.random.Generator`")
    }
}

/// NPY002
pub fn numpy_legacy_random(checker: &mut Checker, expr: &Expr) {
    if let Some(method_name) = checker.ctx.resolve_call_path(expr).and_then(|call_path| {
        // seeding state
        if call_path.as_slice() == ["numpy", "random", "seed"]
            || call_path.as_slice() == ["numpy", "random", "get_state"]
            || call_path.as_slice() == ["numpy", "random", "set_state"]
            // simple random data
            || call_path.as_slice() == ["numpy", "random", "rand"]
            || call_path.as_slice() == ["numpy", "random", "randn"]
            || call_path.as_slice() == ["numpy", "random", "randint"]
            || call_path.as_slice() == ["numpy", "random", "random_integers"]
            || call_path.as_slice() == ["numpy", "random", "random_sample"]
            || call_path.as_slice() == ["numpy", "random", "choice"]
            || call_path.as_slice() == ["numpy", "random", "bytes"]
            // permutations
            || call_path.as_slice() == ["numpy", "random", "shuffle"]
            || call_path.as_slice() == ["numpy", "random", "permutation"]
            // distributions
            || call_path.as_slice() == ["numpy", "random", "beta"]
            || call_path.as_slice() == ["numpy", "random", "binomial"]
            || call_path.as_slice() == ["numpy", "random", "chisquare"]
            || call_path.as_slice() == ["numpy", "random", "dirichlet"]
            || call_path.as_slice() == ["numpy", "random", "exponential"]
            || call_path.as_slice() == ["numpy", "random", "f"]
            || call_path.as_slice() == ["numpy", "random", "gamma"]
            || call_path.as_slice() == ["numpy", "random", "geometric"]
            || call_path.as_slice() == ["numpy", "random", "get_state"]
            || call_path.as_slice() == ["numpy", "random", "gumbel"]
            || call_path.as_slice() == ["numpy", "random", "hypergeometric"]
            || call_path.as_slice() == ["numpy", "random", "laplace"]
            || call_path.as_slice() == ["numpy", "random", "logistic"]
            || call_path.as_slice() == ["numpy", "random", "lognormal"]
            || call_path.as_slice() == ["numpy", "random", "logseries"]
            || call_path.as_slice() == ["numpy", "random", "multinomial"]
            || call_path.as_slice() == ["numpy", "random", "multivariate_normal"]
            || call_path.as_slice() == ["numpy", "random", "negative_binomial"]
            || call_path.as_slice() == ["numpy", "random", "noncentral_chisquare"]
            || call_path.as_slice() == ["numpy", "random", "noncentral_f"]
            || call_path.as_slice() == ["numpy", "random", "normal"]
            || call_path.as_slice() == ["numpy", "random", "pareto"]
            || call_path.as_slice() == ["numpy", "random", "poisson"]
            || call_path.as_slice() == ["numpy", "random", "power"]
            || call_path.as_slice() == ["numpy", "random", "rayleigh"]
            || call_path.as_slice() == ["numpy", "random", "standard_cauchy"]
            || call_path.as_slice() == ["numpy", "random", "standard_exponential"]
            || call_path.as_slice() == ["numpy", "random", "standard_gamma"]
            || call_path.as_slice() == ["numpy", "random", "standard_normal"]
            || call_path.as_slice() == ["numpy", "random", "standard_t"]
            || call_path.as_slice() == ["numpy", "random", "triangular"]
            || call_path.as_slice() == ["numpy", "random", "uniform"]
            || call_path.as_slice() == ["numpy", "random", "vonmises"]
            || call_path.as_slice() == ["numpy", "random", "wald"]
            || call_path.as_slice() == ["numpy", "random", "weibull"]
            || call_path.as_slice() == ["numpy", "random", "zipf"]
        {
            Some(call_path[2])
        } else {
            None
        }
    }) {
        checker.diagnostics.push(Diagnostic::new(
            NumpyLegacyRandom {
                method_name: method_name.to_string(),
            },
            Range::from(expr),
        ));
    }
}
