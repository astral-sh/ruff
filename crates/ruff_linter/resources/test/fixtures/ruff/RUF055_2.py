"""Patterns that don't just involve the call, but rather the parent expression"""
import re

s = "str"

# this should be replaced with `"abc" not in s`
re.search("abc", s) is None


# this should be replaced with `"abc" in s`
re.search("abc", s) is not None


# this should be replaced with `not s.startswith("abc")`
re.match("abc", s) is None


# this should be replaced with `s.startswith("abc")`
re.match("abc", s) is not None


# this should be replaced with `s != "abc"`
re.fullmatch("abc", s) is None


# this should be replaced with `s == "abc"`
re.fullmatch("abc", s) is not None


# this should trigger an unsafe fix because of the presence of a comment within the
# expression being replaced (which we'd lose)
if (
    re.fullmatch(
        "a really really really really long string",
        s,
    )
    # with a comment here
    is None
):
    pass


# this should trigger a safe fix (comments are preserved given they're outside the
# expression)
if (  # leading
    re.fullmatch(
        "a really really really really long string",
        s,
    )
    is None  # trailing
):
    pass
