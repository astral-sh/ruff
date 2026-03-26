# TDO003 - accepted
# TODO: this comment has a link
# https://github.com/astral-sh/ruff/issues/3870

# TODO: this comment has an issue
# TDO-3870

# TODO: this comment has an issue without any code prefix
# this was already existing, but not tested
# 3870

# TODO: the link has an issue code of the minimum length
# T-001

# TODO: the issue code can be arbitrarily long
# ABCDEFGHIJKLMNOPQRSTUVWXYZ-001

# TODO: the issue code can be followed by text
# TD003 is the id of the ticket in our internal jira

# TODO: this works even if we have just a number
# 1234 is the id of the ticket in our internal jira

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
