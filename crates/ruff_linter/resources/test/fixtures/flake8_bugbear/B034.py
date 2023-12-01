import re
from re import sub

# B034
re.sub("a", "b", "aaa", re.IGNORECASE)
re.sub("a", "b", "aaa", 5)
re.sub("a", "b", "aaa", 5, re.IGNORECASE)
re.subn("a", "b", "aaa", re.IGNORECASE)
re.subn("a", "b", "aaa", 5)
re.subn("a", "b", "aaa", 5, re.IGNORECASE)
re.split(" ", "a a a a", re.I)
re.split(" ", "a a a a", 2)
re.split(" ", "a a a a", 2, re.I)
sub("a", "b", "aaa", re.IGNORECASE)

# OK
re.sub("a", "b", "aaa")
re.sub("a", "b", "aaa", flags=re.IGNORECASE)
re.sub("a", "b", "aaa", count=5)
re.sub("a", "b", "aaa", count=5, flags=re.IGNORECASE)
re.subn("a", "b", "aaa")
re.subn("a", "b", "aaa", flags=re.IGNORECASE)
re.subn("a", "b", "aaa", count=5)
re.subn("a", "b", "aaa", count=5, flags=re.IGNORECASE)
re.split(" ", "a a a a", flags=re.I)
re.split(" ", "a a a a", maxsplit=2)
re.split(" ", "a a a a", maxsplit=2, flags=re.I)
