"""Test that RUF055 can follow a chain of str assignments for both the pattern
and the replacement argument to re.sub"""

import re

pat1 = "needle"
pat2 = pat1
pat3 = pat2

re.sub(pat3, "", haystack)

if pat4 := pat3:
    re.sub(pat4, "", haystack)

repl = "new"
re.sub(r"abc", repl, haystack)
