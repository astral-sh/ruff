import re
from re import compile as rec

re.compile(r"hello").match("world")
re.compile("hello world").search("world")
re.compile(r"hello", re.IGNORECASE).findall("world")
re.compile(r"hello", re.I | re.M).finditer("world")
rec(r"hello").match("world")
rec("hello world").search("world")
rec(r"hello", re.IGNORECASE).findall("world")
rec(r"hello", re.I | re.M).finditer("world")


# OK
re.compile(r"hello").match
re.compile("hello world").search
re.compile(r"hello", re.IGNORECASE).findall
re.compile(r"hello", re.I | re.M).finditer
