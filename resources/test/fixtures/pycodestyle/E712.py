#: E712
if res == True:
    pass
#: E712
if res != False:
    pass
#: E712
if True != res:
    pass
#: E712
if False == res:
    pass
#: E712
if res[1] == True:
    pass
#: E712
if res[1] != False:
    pass
#: E712
var = 1 if cond == True else -1 if cond == False else cond
#: E712
if (True) == TrueElement or x == TrueElement:
    pass

if res == True != False:
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

assert (not foo) in bar
assert {"x": not foo} in bar
assert [42, not foo] in bar
