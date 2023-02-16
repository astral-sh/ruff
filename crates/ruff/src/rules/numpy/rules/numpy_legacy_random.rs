use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for legacy NumPy random functions.
    ///
    /// ## Why is this bad?
    /// According to the NumPy documentation the [Legacy Random Generation]:
    /// > The `RandomState` provides access to legacy generators. This generator is considered frozen and will
    /// > have no further improvements. It is guaranteed to produce the same values as the final point release
    /// > of NumPy v1.16. These all depend on Box-Muller normals or inverse CDF exponentials or gammas. This class
    /// > should only be used if it is essential to have randoms that are identical to what would have been produced
    /// > by previous versions of NumPy.
    ///
    /// This is a convenience, legacy function that exists to support older code that uses the singleton `RandomState`.
    /// Best practice is to use a dedicated `Generator` instance rather than the random variate generation methods
    /// exposed directly in the random module.
    ///
    /// The new `Generator` uses bits provided by `PCG64` which by default has better statistical properties than the
    /// legacy `MT19937` used in `RandomState`. See the documentation on [Random Sampling] and [NEP 19] for further details.
    ///
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
    pub struct NumpyLegacyRandom {
        pub method_name: String,
    }
);
impl Violation for NumpyLegacyRandom {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NumpyLegacyRandom { method_name } = self;
        format!("Using legacy method `np.random.{method_name}`, replace with the new random number generator")
    }
}

/// NPY002
pub fn numpy_legacy_random(checker: &mut Checker, expr: &Expr) {
    if let Some(method_name) = checker.resolve_call_path(expr).and_then(|call_path| {
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
            Range::from_located(expr),
        ));
    }
}
