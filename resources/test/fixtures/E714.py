#: E714
if not X is Y:
    pass
#: E714
if not X.B is Y:
    pass
#: E714
if not X is Y is not Z:
    pass

#: Okay
if not X is not Y:
    pass

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
