# setup
sep = ","
no_sep = None

# positives
"""
	itemA
	itemB
	itemC
""".split()

"a,b,c,d".split(",")
"a,b,c,d".split(None)
"a,b,c,d".split(",", 1)
"a,b,c,d".split(None, 1)
"a,b,c,d".split(sep=",")
"a,b,c,d".split(sep=None)
"a,b,c,d".split(sep=",", maxsplit=1)
"a,b,c,d".split(sep=None, maxsplit=1)
"a,b,c,d".split(maxsplit=1, sep=",")
"a,b,c,d".split(maxsplit=1, sep=None)
"a,b,c,d".split(",", maxsplit=1)
"a,b,c,d".split(None, maxsplit=1)
"a,b,c,d".split(maxsplit=1)
"a,b,c,d".split(maxsplit=1.0)
"a,b,c,d".split(maxsplit=1)
"a,b,c,d".split(maxsplit=0)
"VERB AUX PRON ADP DET".split(" ")
'   1   2   3   '.split()
'1<>2<>3<4'.split('<>')

" a*a a*a a ".split("*", -1)  # [" a", "a a", "a a "]
"".split()  # []
"""
""".split()  # []
"   	".split()  # []
"/abc/".split() # ["/abc/"]
("a,b,c"
# comment
.split()
)  # ["a,b,c"]
("a,b,c"
# comment1
.split(",")
) # ["a", "b", "c"]
("a,"
# comment
"b,"
"c"
.split(",")
) # ["a", "b", "c"]

"hello "\
	"world".split()
# ["hello", "world"]

# prefixes and isc
u"a b".split()  # [u"a", u"b"]
r"a \n b".split()  # [r"a", r"\n", r"b"]
("a " "b").split()  # ["a", "b"]
"a " "b".split()  # ["a", "b"]
u"a " "b".split()  # [u"a", u"b"]
"a " u"b".split()  # ["a", "b"]
u"a " r"\n".split()  # [u"a", u"\\n"]
r"\n " u"\n".split()  # [r"\n"]
r"\n " "\n".split()  # [r"\n"]
"a " r"\n".split()  # ["a", "\\n"]

"a,b,c".split(',', maxsplit=0) # ["a,b,c"]
"a,b,c".split(',', maxsplit=-1)  # ["a", "b", "c"]
"a,b,c".split(',', maxsplit=-2)  # ["a", "b", "c"]
"a,b,c".split(',', maxsplit=-0)  # ["a,b,c"]

# negatives

# invalid values should not cause panic
"a,b,c,d".split(maxsplit="hello")
"a,b,c,d".split(maxsplit=-"hello")

# variable names not implemented
"a,b,c,d".split(sep)
"a,b,c,d".split(no_sep)
for n in range(3):
	"a,b,c,d".split(",", maxsplit=n)

# f-strings not yet implemented
world = "world"
_ = f"{world}_hello_world".split("_")

hello = "hello_world"
_ = f"{hello}_world".split("_")

# split on bytes not yet implemented, much less frequent
b"TesT.WwW.ExamplE.CoM".split(b".")

# str.splitlines not yet implemented
"hello\nworld".splitlines()
"hello\nworld".splitlines(keepends=True)
"hello\nworld".splitlines(keepends=False)


# another positive demonstrating quote preservation
"""
"itemA"
'itemB'
'''itemC'''
"'itemD'"
""".split()

# https://github.com/astral-sh/ruff/issues/18042
print("a,b".rsplit(","))
print("a,b,c".rsplit(",", 1))

# https://github.com/astral-sh/ruff/issues/18069

print("".split(maxsplit=0))
print("".split(sep=None, maxsplit=0))
print(" ".split(maxsplit=0))
print(" ".split(sep=None, maxsplit=0))
print(" x ".split(maxsplit=0))
print(" x ".split(sep=None, maxsplit=0))
print("  x  ".split(maxsplit=0))
print("  x  ".split(sep=None, maxsplit=0))
print("".rsplit(maxsplit=0))
print("".rsplit(sep=None, maxsplit=0))
print(" ".rsplit(maxsplit=0))
print(" ".rsplit(sep=None, maxsplit=0))
print(" x ".rsplit(maxsplit=0))
print(" x ".rsplit(maxsplit=0))
print(" x ".rsplit(sep=None, maxsplit=0))
print("  x  ".rsplit(maxsplit=0))
print("  x  ".rsplit(sep=None, maxsplit=0))

# https://github.com/astral-sh/ruff/issues/19581 - embedded quotes in raw strings
r"""simple@example.com
very.common@example.com
FirstName.LastName@EasierReading.org
x@example.com
long.email-address-with-hyphens@and.subdomains.example.com
user.name+tag+sorting@example.com
name/surname@example.com
xample@s.example
" "@example.org
"john..doe"@example.org
mailhost!username@example.org
"very.(),:;<>[]\".VERY.\"very@\\ \"very\".unusual"@strange.example.com
user%example.com@example.org
user-@example.org
I❤️CHOCOLATE@example.com
this\ still\"not\\allowed@example.com
stellyamburrr985@example.com
Abc.123@example.com
user+mailbox/department=shipping@example.com
!#$%&'*+-/=?^_`.{|}~@example.com
"Abc@def"@example.com
"Fred\ Bloggs"@example.com
"Joe.\\Blow"@example.com""".split("\n")


r"""first
'no need' to escape
"swap" quote style
"use' ugly triple quotes""".split("\n")

# https://github.com/astral-sh/ruff/issues/19845
print("S\x1cP\x1dL\x1eI\x1fT".split())
print("\x1c\x1d\x1e\x1f>".split(maxsplit=0))
print("<\x1c\x1d\x1e\x1f".rsplit(maxsplit=0))

# leading/trailing whitespace should not count towards maxsplit
" a b c d ".split(maxsplit=2)  # ["a", "b", "c d "]
" a b c d ".rsplit(maxsplit=2)  # [" a b", "c", "d"]
"a  b".split(maxsplit=1)  # ["a", "b"]
