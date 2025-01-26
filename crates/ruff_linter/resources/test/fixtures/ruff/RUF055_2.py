"""Patterns that don't just involve the call, but rather the parent expression"""
import re

s = "str"

# this should be replaced with `"abc" not in s`
re.search("abc", s) is None


# this shuold be replaced with `"abc" in s`
re.search("abc", s) is not None


# this should be replaced with `not s.startswith("abc")`
re.match("abc", s) is None


# this should be replaced with `s.startswith("abc")`
re.match("abc", s) is not None


# this should be replaced with `s != "abc"`
re.fullmatch("abc", s) is None


# this should be replaced with `s == "abc"`
re.fullmatch("abc", s) is not None
