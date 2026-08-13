# This is a regression test for https://github.com/astral-sh/ruff/issues/19310
# there is a (potentially invisible) unicode formfeed character (000C) between "docstring" and the semicolon
"docstring"; print(
    f"{__doc__=}",
)
