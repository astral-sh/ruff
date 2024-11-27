import re

s = "str"

# this should be replaced with s.replace("abc", "")
re.sub("abc", "", s)


# this example, adapted from https://docs.python.org/3/library/re.html#re.sub,
# should *not* be replaced because repl is a function, not a string
def dashrepl(matchobj):
    if matchobj.group(0) == "-":
        return " "
    else:
        return "-"


re.sub("-", dashrepl, "pro----gram-files")

# this one should be replaced with s.startswith("abc") because the Match is
# used in an if context for its truth value
if re.match("abc", s):
    pass
if m := re.match("abc", s):  # this should *not* be replaced
    pass
re.match("abc", s)  # this should not be replaced because match returns a Match

# this should be replaced with "abc" in s
if re.search("abc", s):
    pass
re.search("abc", s)  # this should not be replaced

# this should be replaced with "abc" == s
if re.fullmatch("abc", s):
    pass
re.fullmatch("abc", s)  # this should not be replaced

# this should be replaced with s.split("abc")
re.split("abc", s)

# these currently should not be modified because the patterns contain regex
# metacharacters
re.sub("ab[c]", "", s)
re.match("ab[c]", s)
re.search("ab[c]", s)
re.fullmatch("ab[c]", s)
re.split("ab[c]", s)
