"""A module docstring with D214 violations

    Returns
-----
    valid returns

    Args
-----
    valid args
"""

import os
from .expected import Expectation

expectation = Expectation()
expect = expectation.expect

expect(os.path.normcase(__file__ if __file__[-1] != 'c' else __file__[:-1]),
       "D214: Section is over-indented ('Returns')")
expect(os.path.normcase(__file__ if __file__[-1] != 'c' else __file__[:-1]),
       "D214: Section is over-indented ('Args')")