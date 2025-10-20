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

# Valid Jira-style patterns (should pass)
# TODO: Move this part to the other place RFFU-6877
# TODO: PROJ-123 Another Jira-style example
# TODO: Fix bug ABC-123
# TODO: Implement feature XYZ-456
# TODO: Update documentation DEF-789
# TODO: Refactor code GHI-101112
# TODO: Fix this (AIRFLOW-123)
# TODO: Update config (SUPERSET-456)

# Invalid patterns that should still trigger TD003 (should fail)
# TODO: Single letter project key A-1
# TODO: No hyphen pattern ABC123
# TODO: Lowercase project key abc-123
# TODO: Mixed case project key AbC-123
# TODO: Just a random word with hyphen and number random-123
# TODO: Number before letters 123-ABC
# TODO: Multiple hyphens ABC-123-456
# TODO: Empty project key -123
# TODO: This is about PROJ-123 server config (ID in middle of sentence)
# TODO: Working on PROJ-123 and PROJ-456 (multiple IDs)
