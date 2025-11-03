"""Random variable generators.

    bytes
    -----
           uniform bytes (values between 0 and 255)

    integers
    --------
           uniform within range

    sequences
    ---------
           pick random element
           pick random sample
           pick weighted random sample
           generate random permutation

    distributions on the real line:
    ------------------------------
           uniform
           triangular
           normal (Gaussian)
           lognormal
           negative exponential
           gamma
           beta
           pareto
           Weibull

    distributions on the circle (angles 0 to 2pi)
    ---------------------------------------------
           circular uniform
           von Mises

    discrete distributions
    ----------------------
           binomial


General notes on the underlying Mersenne Twister core generator:

* The period is 2**19937-1.
* It is one of the most extensively tested generators in existence.
* The random() method is implemented in C, executes in a single Python step,
  and is, therefore, threadsafe.

"""

import _random
import sys
from _typeshed import SupportsLenAndGetItem
from collections.abc import Callable, Iterable, MutableSequence, Sequence, Set as AbstractSet
from fractions import Fraction
from typing import Any, ClassVar, NoReturn, TypeVar
from typing_extensions import Self

__all__ = [
    "Random",
    "seed",
    "random",
    "uniform",
    "randint",
    "choice",
    "sample",
    "randrange",
    "shuffle",
    "normalvariate",
    "lognormvariate",
    "expovariate",
    "vonmisesvariate",
    "gammavariate",
    "triangular",
    "gauss",
    "betavariate",
    "paretovariate",
    "weibullvariate",
    "getstate",
    "setstate",
    "getrandbits",
    "choices",
    "SystemRandom",
    "randbytes",
]

if sys.version_info >= (3, 12):
    __all__ += ["binomialvariate"]

_T = TypeVar("_T")

class Random(_random.Random):
    """Random number generator base class used by bound module functions.

    Used to instantiate instances of Random to get generators that don't
    share state.

    Class Random can also be subclassed if you want to use a different basic
    generator of your own devising: in that case, override the following
    methods:  random(), seed(), getstate(), and setstate().
    Optionally, implement a getrandbits() method so that randrange()
    can cover arbitrarily large ranges.

    """

    VERSION: ClassVar[int]
    def __init__(self, x: int | float | str | bytes | bytearray | None = None) -> None:  # noqa: Y041
        """Initialize an instance.

        Optional argument x controls seeding, as for Random.seed().
        """
    # Using other `seed` types is deprecated since 3.9 and removed in 3.11
    # Ignore Y041, since random.seed doesn't treat int like a float subtype. Having an explicit
    # int better documents conventional usage of random.seed.
    if sys.version_info < (3, 10):
        # this is a workaround for pyright correctly flagging an inconsistent inherited constructor, see #14624
        def __new__(cls, x: int | float | str | bytes | bytearray | None = None) -> Self: ...  # noqa: Y041

    def seed(self, a: int | float | str | bytes | bytearray | None = None, version: int = 2) -> None:  # type: ignore[override]  # noqa: Y041
        """Initialize internal state from a seed.

        The only supported seed types are None, int, float,
        str, bytes, and bytearray.

        None or no argument seeds from current time or from an operating
        system specific randomness source if available.

        If *a* is an int, all bits are used.

        For version 2 (the default), all of the bits are used if *a* is a str,
        bytes, or bytearray.  For version 1 (provided for reproducing random
        sequences from older versions of Python), the algorithm for str and
        bytes generates a narrower range of seeds.

        """

    def getstate(self) -> tuple[Any, ...]:
        """Return internal state; can be passed to setstate() later."""

    def setstate(self, state: tuple[Any, ...]) -> None:
        """Restore internal state from object returned by getstate()."""

    def randrange(self, start: int, stop: int | None = None, step: int = 1) -> int:
        """Choose a random item from range(stop) or range(start, stop[, step]).

        Roughly equivalent to ``choice(range(start, stop, step))`` but
        supports arbitrarily large ranges and is optimized for common cases.

        """

    def randint(self, a: int, b: int) -> int:
        """Return random integer in range [a, b], including both end points."""

    def randbytes(self, n: int) -> bytes:
        """Generate n random bytes."""

    def choice(self, seq: SupportsLenAndGetItem[_T]) -> _T:
        """Choose a random element from a non-empty sequence."""

    def choices(
        self,
        population: SupportsLenAndGetItem[_T],
        weights: Sequence[float | Fraction] | None = None,
        *,
        cum_weights: Sequence[float | Fraction] | None = None,
        k: int = 1,
    ) -> list[_T]:
        """Return a k sized list of population elements chosen with replacement.

        If the relative weights or cumulative weights are not specified,
        the selections are made with equal probability.

        """
    if sys.version_info >= (3, 11):
        def shuffle(self, x: MutableSequence[Any]) -> None:
            """Shuffle list x in place, and return None."""
    else:
        def shuffle(self, x: MutableSequence[Any], random: Callable[[], float] | None = None) -> None:
            """Shuffle list x in place, and return None.

            Optional argument random is a 0-argument function returning a
            random float in [0.0, 1.0); if it is the default None, the
            standard random.random will be used.

            """
    if sys.version_info >= (3, 11):
        def sample(self, population: Sequence[_T], k: int, *, counts: Iterable[int] | None = None) -> list[_T]:
            """Chooses k unique random elements from a population sequence.

            Returns a new list containing elements from the population while
            leaving the original population unchanged.  The resulting list is
            in selection order so that all sub-slices will also be valid random
            samples.  This allows raffle winners (the sample) to be partitioned
            into grand prize and second place winners (the subslices).

            Members of the population need not be hashable or unique.  If the
            population contains repeats, then each occurrence is a possible
            selection in the sample.

            Repeated elements can be specified one at a time or with the optional
            counts parameter.  For example:

                sample(['red', 'blue'], counts=[4, 2], k=5)

            is equivalent to:

                sample(['red', 'red', 'red', 'red', 'blue', 'blue'], k=5)

            To choose a sample from a range of integers, use range() for the
            population argument.  This is especially fast and space efficient
            for sampling from a large population:

                sample(range(10000000), 60)

            """
    else:
        def sample(self, population: Sequence[_T] | AbstractSet[_T], k: int, *, counts: Iterable[int] | None = None) -> list[_T]:
            """Chooses k unique random elements from a population sequence or set.

            Returns a new list containing elements from the population while
            leaving the original population unchanged.  The resulting list is
            in selection order so that all sub-slices will also be valid random
            samples.  This allows raffle winners (the sample) to be partitioned
            into grand prize and second place winners (the subslices).

            Members of the population need not be hashable or unique.  If the
            population contains repeats, then each occurrence is a possible
            selection in the sample.

            Repeated elements can be specified one at a time or with the optional
            counts parameter.  For example:

                sample(['red', 'blue'], counts=[4, 2], k=5)

            is equivalent to:

                sample(['red', 'red', 'red', 'red', 'blue', 'blue'], k=5)

            To choose a sample from a range of integers, use range() for the
            population argument.  This is especially fast and space efficient
            for sampling from a large population:

                sample(range(10000000), 60)

            """

    def uniform(self, a: float, b: float) -> float:
        """Get a random number in the range [a, b) or [a, b] depending on rounding.

        The mean (expected value) and variance of the random variable are:

            E[X] = (a + b) / 2
            Var[X] = (b - a) ** 2 / 12

        """

    def triangular(self, low: float = 0.0, high: float = 1.0, mode: float | None = None) -> float:
        """Triangular distribution.

        Continuous distribution bounded by given lower and upper limits,
        and having a given mode value in-between.

        http://en.wikipedia.org/wiki/Triangular_distribution

        The mean (expected value) and variance of the random variable are:

            E[X] = (low + high + mode) / 3
            Var[X] = (low**2 + high**2 + mode**2 - low*high - low*mode - high*mode) / 18

        """
    if sys.version_info >= (3, 12):
        def binomialvariate(self, n: int = 1, p: float = 0.5) -> int:
            """Binomial random variable.

            Gives the number of successes for *n* independent trials
            with the probability of success in each trial being *p*:

                sum(random() < p for i in range(n))

            Returns an integer in the range:

                0 <= X <= n

            The integer is chosen with the probability:

                P(X == k) = math.comb(n, k) * p ** k * (1 - p) ** (n - k)

            The mean (expected value) and variance of the random variable are:

                E[X] = n * p
                Var[X] = n * p * (1 - p)

            """

    def betavariate(self, alpha: float, beta: float) -> float:
        """Beta distribution.

        Conditions on the parameters are alpha > 0 and beta > 0.
        Returned values range between 0 and 1.

        The mean (expected value) and variance of the random variable are:

            E[X] = alpha / (alpha + beta)
            Var[X] = alpha * beta / ((alpha + beta)**2 * (alpha + beta + 1))

        """
    if sys.version_info >= (3, 12):
        def expovariate(self, lambd: float = 1.0) -> float:
            """Exponential distribution.

            lambd is 1.0 divided by the desired mean.  It should be
            nonzero.  (The parameter would be called "lambda", but that is
            a reserved word in Python.)  Returned values range from 0 to
            positive infinity if lambd is positive, and from negative
            infinity to 0 if lambd is negative.

            The mean (expected value) and variance of the random variable are:

                E[X] = 1 / lambd
                Var[X] = 1 / lambd ** 2

            """
    else:
        def expovariate(self, lambd: float) -> float:
            """Exponential distribution.

            lambd is 1.0 divided by the desired mean.  It should be
            nonzero.  (The parameter would be called "lambda", but that is
            a reserved word in Python.)  Returned values range from 0 to
            positive infinity if lambd is positive, and from negative
            infinity to 0 if lambd is negative.

            """

    def gammavariate(self, alpha: float, beta: float) -> float:
        """Gamma distribution.  Not the gamma function!

        Conditions on the parameters are alpha > 0 and beta > 0.

        The probability distribution function is:

                    x ** (alpha - 1) * math.exp(-x / beta)
          pdf(x) =  --------------------------------------
                      math.gamma(alpha) * beta ** alpha

        The mean (expected value) and variance of the random variable are:

            E[X] = alpha * beta
            Var[X] = alpha * beta ** 2

        """
    if sys.version_info >= (3, 11):
        def gauss(self, mu: float = 0.0, sigma: float = 1.0) -> float:
            """Gaussian distribution.

            mu is the mean, and sigma is the standard deviation.  This is
            slightly faster than the normalvariate() function.

            Not thread-safe without a lock around calls.

            """

        def normalvariate(self, mu: float = 0.0, sigma: float = 1.0) -> float:
            """Normal distribution.

            mu is the mean, and sigma is the standard deviation.

            """
    else:
        def gauss(self, mu: float, sigma: float) -> float:
            """Gaussian distribution.

            mu is the mean, and sigma is the standard deviation.  This is
            slightly faster than the normalvariate() function.

            Not thread-safe without a lock around calls.

            """

        def normalvariate(self, mu: float, sigma: float) -> float:
            """Normal distribution.

            mu is the mean, and sigma is the standard deviation.

            """

    def lognormvariate(self, mu: float, sigma: float) -> float:
        """Log normal distribution.

        If you take the natural logarithm of this distribution, you'll get a
        normal distribution with mean mu and standard deviation sigma.
        mu can have any value, and sigma must be greater than zero.

        """

    def vonmisesvariate(self, mu: float, kappa: float) -> float:
        """Circular data distribution.

        mu is the mean angle, expressed in radians between 0 and 2*pi, and
        kappa is the concentration parameter, which must be greater than or
        equal to zero.  If kappa is equal to zero, this distribution reduces
        to a uniform random angle over the range 0 to 2*pi.

        """

    def paretovariate(self, alpha: float) -> float:
        """Pareto distribution.  alpha is the shape parameter."""

    def weibullvariate(self, alpha: float, beta: float) -> float:
        """Weibull distribution.

        alpha is the scale parameter and beta is the shape parameter.

        """

# SystemRandom is not implemented for all OS's; good on Windows & Linux
class SystemRandom(Random):
    """Alternate random number generator using sources provided
    by the operating system (such as /dev/urandom on Unix or
    CryptGenRandom on Windows).

     Not available on all systems (see os.urandom() for details).

    """

    def getrandbits(self, k: int) -> int:  # k can be passed by keyword
        """getrandbits(k) -> x.  Generates an int with k random bits."""

    def getstate(self, *args: Any, **kwds: Any) -> NoReturn:
        """Method should not be called for a system random number generator."""

    def setstate(self, *args: Any, **kwds: Any) -> NoReturn:
        """Method should not be called for a system random number generator."""

_inst: Random
seed = _inst.seed
random = _inst.random
uniform = _inst.uniform
triangular = _inst.triangular
randint = _inst.randint
choice = _inst.choice
randrange = _inst.randrange
sample = _inst.sample
shuffle = _inst.shuffle
choices = _inst.choices
normalvariate = _inst.normalvariate
lognormvariate = _inst.lognormvariate
expovariate = _inst.expovariate
vonmisesvariate = _inst.vonmisesvariate
gammavariate = _inst.gammavariate
gauss = _inst.gauss
if sys.version_info >= (3, 12):
    binomialvariate = _inst.binomialvariate
betavariate = _inst.betavariate
paretovariate = _inst.paretovariate
weibullvariate = _inst.weibullvariate
getstate = _inst.getstate
setstate = _inst.setstate
getrandbits = _inst.getrandbits
randbytes = _inst.randbytes
