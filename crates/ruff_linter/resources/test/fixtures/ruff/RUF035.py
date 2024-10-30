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

# negatives

# test 
"a,b,c,d".split(maxsplit="hello")

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
