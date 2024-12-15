# This test is in its own file because it results in AST changes on stable.
# We don't plan to fix it on stable because we plan on promoting f-string formatting soon.

# Regression test for https://github.com/astral-sh/ruff/issues/14766
# Don't change the casing of the escaped characters in the f-string because it would be observable in the debug expression.
# >>> f"{r"\xFF"=}"
# 'r"ÿ"=\'\\\\xFF\''
# >>> f"{r"\xff"=}"
# 'r"ÿ"=\'\\\\xff\''
f"{r"\xFF"=}"
f"{r"\xff"=}"


# Test for https://github.com/astral-sh/ruff/issues/14926
# We could change the casing of the escaped characters in non-raw f-string because Python interprets the inner string
# before printing it as a debug expression. For now, we decided not to change the casing because it's fairly complicated
# to implement and it seems uncommon (debug expressions are uncommon, escapes are uncommon).
# These two strings could be formatted to the same output but we currently don't do that.
f"{"\xFF\N{space}"=}"
f"{"\xff\N{SPACE}"=}"
