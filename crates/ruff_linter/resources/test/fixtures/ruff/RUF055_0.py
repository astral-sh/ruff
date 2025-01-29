import re

s = "str"

# this should be replaced with `s.replace("abc", "")`
re.sub("abc", "", s)


# this example, adapted from https://docs.python.org/3/library/re.html#re.sub,
# should *not* be replaced because repl is a function, not a string
def dashrepl(matchobj):
    if matchobj.group(0) == "-":
        return " "
    else:
        return "-"


re.sub("-", dashrepl, "pro----gram-files")

# this one should be replaced with `s.startswith("abc")` because the Match is
# used in an if context for its truth value
if re.match("abc", s):
    pass
if m := re.match("abc", s):  # this should *not* be replaced
    pass
re.match("abc", s)  # this should not be replaced because match returns a Match

# this should be replaced with `"abc" in s`
if re.search("abc", s):
    pass
re.search("abc", s)  # this should not be replaced

# this should be replaced with `"abc" == s`
if re.fullmatch("abc", s):
    pass
re.fullmatch("abc", s)  # this should not be replaced

# this should be replaced with `s.split("abc")`
re.split("abc", s)

# these currently should not be modified because the patterns contain regex
# metacharacters
re.sub("ab[c]", "", s)
re.match("ab[c]", s)
re.search("ab[c]", s)
re.fullmatch("ab[c]", s)
re.split("ab[c]", s)

# test that all of the metacharacters prevent the rule from triggering, also
# use raw strings in line with RUF039
re.sub(r"abc.", "", s)
re.sub(r"^abc", "", s)
re.sub(r"abc$", "", s)
re.sub(r"abc*", "", s)
re.sub(r"abc+", "", s)
re.sub(r"abc?", "", s)
re.sub(r"abc{2,3}", "", s)
re.sub(r"abc\n", "", s)  # this one could be fixed but is not currently
re.sub(r"abc|def", "", s)
re.sub(r"(a)bc", "", s)
re.sub(r"a)bc", "", s)  # https://github.com/astral-sh/ruff/issues/15316

# and these should not be modified because they have extra arguments
re.sub("abc", "", s, flags=re.A)
re.match("abc", s, flags=re.I)
re.search("abc", s, flags=re.L)
re.fullmatch("abc", s, flags=re.M)
re.split("abc", s, maxsplit=2)

# this should trigger an unsafe fix because of the presence of comments
re.sub(
    # pattern
    "abc",
    # repl
    "",
    s,  # string
)

# A diagnostic should not be emitted for `sub` replacements with backreferences or
# most other ASCII escapes
re.sub(r"a", r"\g<0>\g<0>\g<0>", "a")
re.sub(r"a", r"\1", "a")
re.sub(r"a", r"\s", "a")

# Escapes like \n are "processed":
# `re.sub(r"a", r"\n", some_string)` is fixed to `some_string.replace("a", "\n")`
# *not* `some_string.replace("a", "\\n")`.
# We currently emit diagnostics for some of these without fixing them.
re.sub(r"a", "\n", "a")
re.sub(r"a", r"\n", "a")
re.sub(r"a", "\a", "a")
re.sub(r"a", r"\a", "a")

re.sub(r"a", "\?", "a")
re.sub(r"a", r"\?", "a")

# these double as tests for preserving raw string quoting style
re.sub(r'abc', "", s)
re.sub(r"""abc""", "", s)
re.sub(r'''abc''', "", s)
