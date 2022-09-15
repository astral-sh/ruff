#: E711
if res == None:
    pass
#: E711
if res != None:
    pass
#: E711
if None == res:
    pass
#: E711
if None != res:
    pass
#: E711
if res[1] == None:
    pass
#: E711
if res[1] != None:
    pass
#: E711
if None != res[1]:
    pass
#: E711
if None == res[1]:
    pass

#: Okay
if x not in y:
    pass

if not (X in Y or X is Z):
    pass

if not (X in Y):
    pass

if x is not y:
    pass

if X is not Y is not Z:
    pass

if TrueElement.get_element(True) == TrueElement.get_element(False):
    pass

if (True) == TrueElement or x == TrueElement:
    pass

assert (not foo) in bar
assert {"x": not foo} in bar
assert [42, not foo] in bar
