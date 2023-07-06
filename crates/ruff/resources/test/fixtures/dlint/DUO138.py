import re

from re import compile, search, match, fullmatch, split, findall, finditer, sub, subn

re.compile("(a+)+b")  # DUO138
re.search("(a+)+b")  # DUO138
re.match("(a+)+b")  # DUO138
re.fullmatch("(a+)+b")  # DUO138
re.split("(a+)+b")  # DUO138
re.findall("(a+)+b")  # DUO138
re.finditer("(a+)+b")  # DUO138
re.sub("(a+)+b")  # DUO138
re.subn("(a+)+b")  # DUO138

compile("(a+)+b")  # DUO138
search("(a+)+b")  # DUO138
match("(a+)+b")  # DUO138
fullmatch("(a+)+b")  # DUO138
split("(a+)+b")  # DUO138
findall("(a+)+b")  # DUO138
finditer("(a+)+b")  # DUO138
sub("(a+)+b")  # DUO138
subn("(a+)+b")  # DUO138


re.search("(a+)+?b")  # DUO138

re.search("(a+)*b")  # DUO138

re.search("(a+)*?b")  # DUO138

re.search("(a+){1,10}b")  # DUO138

re.search("(a+){1,10}?b")  # DUO138

re.search("(a+){10}b")  # DUO138

re.search("(a+){10}?b")  # DUO138

re.search("(a+){,10}b")  # DUO138

re.search("(a+){,10}?b")  # DUO138

re.search("(a+){10,}b")  # DUO138

re.search("(a+){10,}?b")  # DUO138

re.search("(a+?)+b")  # DUO138

re.search("(a+?)*b")  # DUO138

re.search("(a+?)*?b")  # DUO138

re.search("(a+?){1,10}b")  # DUO138

re.search("(a+?){1,10}?b")  # DUO138

re.search("(a+?){10}b")  # DUO138

re.search("(a+?){10}?b")  # DUO138

re.search("(a+?){,10}b")  # DUO138

re.search("(a+?){,10}?b")  # DUO138

re.search("(a+?){10,}b")  # DUO138

re.search("(a+?){10,}?b")  # DUO138

re.search("(a*)+b")  # DUO138

re.search("(a*)+?b")  # DUO138

re.search("(a*)*?b")  # DUO138

re.search("(a*){1,10}b")  # DUO138

re.search("(a*){1,10}?b")  # DUO138

re.search("(a*){10}b")  # DUO138

re.search("(a*){10}?b")  # DUO138

re.search("(a*){,10}b")  # DUO138

re.search("(a*){,10}?b")  # DUO138

re.search("(a*){10,}b")  # DUO138

re.search("(a*){10,}?b")  # DUO138

re.search("(a*?)+b")  # DUO138

re.search("(a*?)+?b")  # DUO138

re.search("(a*?)*b")  # DUO138

re.search("(a*?){1,10}b")  # DUO138

re.search("(a*?){1,10}?b")  # DUO138

re.search("(a*?){10}b")  # DUO138

re.search("(a*?){10}?b")  # DUO138

re.search("(a*?){,10}b")  # DUO138

re.search("(a*?){,10}?b")  # DUO138

re.search("(a*?){10,}b")  # DUO138

re.search("(a*?){10,}?b")  # DUO138

re.search("(a{1,10})+b")  # DUO138

re.search("(a{1,10})+?b")  # DUO138

re.search("(a{1,10})*b")  # DUO138

re.search("(a{1,10})*?b")  # DUO138

re.search("(a{1,10}){1,10}?b")  # DUO138

re.search("(a{1,10}){10}b")  # DUO138

re.search("(a{1,10}){10}?b")  # DUO138

re.search("(a{1,10}){,10}b")  # DUO138

re.search("(a{1,10}){,10}?b")  # DUO138

re.search("(a{1,10}){10,}b")  # DUO138

re.search("(a{1,10}){10,}?b")  # DUO138

re.search("(a{1,10}?)+b")  # DUO138

re.search("(a{1,10}?)+?b")  # DUO138

re.search("(a{1,10}?)*b")  # DUO138

re.search("(a{1,10}?)*?b")  # DUO138

re.search("(a{1,10}?){1,10}b")  # DUO138

re.search("(a{1,10}?){10}b")  # DUO138

re.search("(a{1,10}?){10}?b")  # DUO138

re.search("(a{1,10}?){,10}b")  # DUO138

re.search("(a{1,10}?){,10}?b")  # DUO138

re.search("(a{1,10}?){10,}b")  # DUO138

re.search("(a{1,10}?){10,}?b")  # DUO138

re.search("(a{10})+b")  # DUO138

re.search("(a{10})+?b")  # DUO138

re.search("(a{10})*b")  # DUO138

re.search("(a{10})*?b")  # DUO138

re.search("(a{10}){1,10}b")  # DUO138

re.search("(a{10}){1,10}?b")  # DUO138

re.search("(a{10}){10}?b")  # DUO138

re.search("(a{10}){,10}b")  # DUO138

re.search("(a{10}){,10}?b")  # DUO138

re.search("(a{10}){10,}b")  # DUO138

re.search("(a{10}){10,}?b")  # DUO138

re.search("(a{10}?)+b")  # DUO138

re.search("(a{10}?)+?b")  # DUO138

re.search("(a{10}?)*b")  # DUO138

re.search("(a{10}?)*?b")  # DUO138

re.search("(a{10}?){1,10}b")  # DUO138

re.search("(a{10}?){1,10}?b")  # DUO138

re.search("(a{10}?){10}b")  # DUO138

re.search("(a{10}?){,10}b")  # DUO138

re.search("(a{10}?){,10}?b")  # DUO138

re.search("(a{10}?){10,}b")  # DUO138

re.search("(a{10}?){10,}?b")  # DUO138

re.search("(a{,10})+b")  # DUO138

re.search("(a{,10})+?b")  # DUO138

re.search("(a{,10})*b")  # DUO138

re.search("(a{,10})*?b")  # DUO138

re.search("(a{,10}){1,10}b")  # DUO138

re.search("(a{,10}){1,10}?b")  # DUO138

re.search("(a{,10}){10}b")  # DUO138

re.search("(a{,10}){10}?b")  # DUO138

re.search("(a{,10}){,10}?b")  # DUO138

re.search("(a{,10}){10,}b")  # DUO138

re.search("(a{,10}){10,}?b")  # DUO138

re.search("(a{,10}?)+b")  # DUO138

re.search("(a{,10}?)+?b")  # DUO138

re.search("(a{,10}?)*b")  # DUO138

re.search("(a{,10}?)*?b")  # DUO138

re.search("(a{,10}?){1,10}b")  # DUO138

re.search("(a{,10}?){1,10}?b")  # DUO138

re.search("(a{,10}?){10}b")  # DUO138

re.search("(a{,10}?){10}?b")  # DUO138

re.search("(a{,10}?){,10}b")  # DUO138

re.search("(a{,10}?){10,}b")  # DUO138

re.search("(a{,10}?){10,}?b")  # DUO138

re.search("(a{10,})+b")  # DUO138

re.search("(a{10,})+?b")  # DUO138

re.search("(a{10,})*b")  # DUO138

re.search("(a{10,})*?b")  # DUO138

re.search("(a{10,}){1,10}b")  # DUO138

re.search("(a{10,}){1,10}?b")  # DUO138

re.search("(a{10,}){10}b")  # DUO138

re.search("(a{10,}){10}?b")  # DUO138

re.search("(a{10,}){,10}b")  # DUO138

re.search("(a{10,}){,10}?b")  # DUO138

re.search("(a{10,}){10,}?b")  # DUO138

re.search("(a{10,}?)+b")  # DUO138

re.search("(a{10,}?)+?b")  # DUO138

re.search("(a{10,}?)*b")  # DUO138

re.search("(a{10,}?)*?b")  # DUO138

re.search("(a{10,}?){1,10}b")  # DUO138

re.search("(a{10,}?){1,10}?b")  # DUO138

re.search("(a{10,}?){10}b")  # DUO138

re.search("(a{10,}?){10}?b")  # DUO138

re.search("(a{10,}?){,10}b")  # DUO138

re.search("(a{10,}?){,10}?b")  # DUO138

re.search("(a{10,}?){10,}b")  # DUO138

re.search("[abc]+[def]*")  # OK

re.search("[abc]+([def]*)")  # OK

re.search("(.|[a-c])+")  # DUO138

re.search("(a|[a-c])+")  # DUO138

re.search("(d|[a-c])+")  # OK

re.search("([^d]|[a-c])+")  # DUO138

re.search("([^b]|[a-c])+")  # DUO138

re.search("([^a]|a)+")  # OK

re.search("([^d]|[^b]|[a-c])+")  # DUO138


re.search("([^abcAB]|[a-c]|[A-C])+")  # DUO138

re.search("([^abcABC]|[a-c]|[A-C])+")  # OK


re.search("([a-c]|[c-e])+")  # DUO138

re.search("([a-c]|[d-e])+")  # OK
