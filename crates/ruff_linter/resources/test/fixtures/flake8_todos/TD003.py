# TDO003 - accepted
# TODO: this comment has a link
# https://github.com/astral-sh/ruff/issues/3870

# TODO: this comment has an issue
# TDO-3870

# TODO: the link has an issue code of the minimum length
# T-001

# TODO: the issue code can be arbitrarily long
# ABCDEFGHIJKLMNOPQRSTUVWXYZ-001

# TDO003 - errors
# TODO: this comment has no
# link after it

# TODO: here's a TODO with no link after it
def foo(x):
    return x

# TODO: here's a TODO on the last line with no link
# Here's more content.
# TDO-3870

# TODO: here's a TODO on the last line with no link
# Here's more content, with a space.

# TDO-3870

# TODO: here's a TODO without an issue link
# TODO: followed by a new TODO with an issue link
# TDO-3870

# foo # TODO: no link!

# TODO: https://github.com/astral-sh/ruff/pull/9627 A todo with a link on the same line

# TODO: #9627 A todo with a number on the same line

# TODO: A todo with a random number, 5431

# TODO: here's a TODO on the last line with no link

# Jira-style issue IDs on the same line (should pass)
# TODO(charlie): PROJ-123 fix the widget
# TODO(charlie): Fix bug (AIRFLOW-456)
# TODO(charlie): Update config SUPERSET-789
# TODO(charlie): ABC-456: fix bug with colon

# Jira-style patterns that should still fail
# TODO(charlie): Single letter project key A-1
# TODO(charlie): No hyphen pattern ABC123
# TODO(charlie): Lowercase project key abc-123
