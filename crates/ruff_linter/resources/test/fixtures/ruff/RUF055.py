import re

s = "str"

# this should be replaced with s.replace("abc", "")
re.sub("abc", "", s)

# this should be replaced with s.startswith("abc")
re.match("abc", s)

# this should be replaced with "abc" in s
re.search("abc", s)

# this should be replaced with "abc" == s
re.fullmatch("abc", s)

# this should be replaced with s.split("abc")
re.split("abc", s)

# these currently should not be modified because the patterns contain regex
# metacharacters
re.sub("ab[c]", "", s)
re.match("ab[c]", s)
re.search("ab[c]", s)
re.fullmatch("ab[c]", s)
re.split("ab[c]", s)
