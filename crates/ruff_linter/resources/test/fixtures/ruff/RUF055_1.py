"""Test that RUF055 can follow a single str assignment for both the pattern and
the replacement argument to re.sub
"""

import re

pat1 = "needle"

re.sub(pat1, "", haystack)

# aliases are not followed, so this one should not trigger the rule
if pat4 := pat1:
    re.sub(pat4, "", haystack)

# also works for the `repl` argument in sub
repl = "new"
re.sub(r"abc", repl, haystack)
