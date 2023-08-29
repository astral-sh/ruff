use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

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
    method_name: String,
}

impl Violation for NumpyLegacyRandom {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NumpyLegacyRandom { method_name } = self;
        format!("Replace legacy `np.random.{method_name}` call with `np.random.Generator`")
    }
}

/// NPY002
pub(crate) fn legacy_random(checker: &mut Checker, expr: &Expr) {
    if let Some(method_name) = checker
        .semantic()
        .resolve_call_path(expr)
        .and_then(|call_path| {
            // seeding state
            if matches!(
                call_path.as_slice(),
                [
                    "numpy",
                    "random",
                    // Seeds
                    "seed" |
                    "get_state" |
                    "set_state" |
                    // Simple random data
                    "rand" |
                    "randn" |
                    "randint" |
                    "random_integers" |
                    "random_sample" |
                    "choice" |
                    "bytes" |
                    // Permutations
                    "shuffle" |
                    "permutation" |
                    // Distributions
                    "beta" |
                    "binomial" |
                    "chisquare" |
                    "dirichlet" |
                    "exponential" |
                    "f" |
                    "gamma" |
                    "geometric" |
                    "gumbel" |
                    "hypergeometric" |
                    "laplace" |
                    "logistic" |
                    "lognormal" |
                    "logseries" |
                    "multinomial" |
                    "multivariate_normal" |
                    "negative_binomial" |
                    "noncentral_chisquare" |
                    "noncentral_f" |
                    "normal" |
                    "pareto" |
                    "poisson" |
                    "power" |
                    "rayleigh" |
                    "standard_cauchy" |
                    "standard_exponential" |
                    "standard_gamma" |
                    "standard_normal" |
                    "standard_t" |
                    "triangular" |
                    "uniform" |
                    "vonmises" |
                    "wald" |
                    "weibull" |
                    "zipf"
                ]
            ) {
                Some(call_path[2])
            } else {
                None
            }
        })
    {
        checker.diagnostics.push(Diagnostic::new(
            NumpyLegacyRandom {
                method_name: method_name.to_string(),
            },
            expr.range(),
        ));
    }
}
