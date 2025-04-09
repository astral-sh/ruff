assert 1 < 2
assert call()
assert a and b
assert lambda x: y
def _(): assert await x
assert x if True else y

assert x, "error"
assert x, lambda x: y
def _(): assert x, await x
assert x, x if True else y
