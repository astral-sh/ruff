# SIM108
if a:
    b = c
else:
    b = d

# OK
b = c if a else d

# OK
if a:
    b = c
elif c:
    b = a
else:
    b = d

# OK
if True:
    pass
elif a:
    b = 1
else:
    b = 2

# SIM108
if True:
    pass
else:
    if a:
        b = 1
    else:
        b = 2


import sys

# OK
if sys.version_info >= (3, 9):
    randbytes = random.randbytes
else:
    randbytes = _get_random_bytes

# OK
if sys.platform == "darwin":
    randbytes = random.randbytes
else:
    randbytes = _get_random_bytes

# OK
if sys.platform.startswith("linux"):
    randbytes = random.randbytes
else:
    randbytes = _get_random_bytes


# SIM108 (without fix due to comments)
if x > 0:
    # test test
    abc = x
else:
    # test test test
    abc = -x


# OK (too long)
if parser.errno == BAD_FIRST_LINE:
    req = wrappers.Request(sock, server=self._server)
else:
    req = wrappers.Request(
        sock,
        parser.get_method(),
        parser.get_scheme() or _scheme,
        parser.get_path(),
        parser.get_version(),
        parser.get_query_string(),
        server=self._server,
    )


# SIM108
if a:
    b = "cccccccccccccccccccccccccccccccccß"
else:
    b = "ddddddddddddddddddddddddddddddddd💣"


# OK (too long)
if True:
    if a:
        b = ccccccccccccccccccccccccccccccccccc
    else:
        b = ddddddddddddddddddddddddddddddddddd


# OK (too long with tabs)
if True:
	if a:
		b = ccccccccccccccccccccccccccccccccccc
	else:
		b = ddddddddddddddddddddddddddddddddddd


# SIM108 (without fix due to trailing comment)
if True:
    exitcode = 0
else:
    exitcode = 1  # Trailing comment


# SIM108
if True: x = 3  # Foo
else: x = 5


# SIM108
if True:  # Foo
    x = 3
else:
    x = 5


# OK
def f():
    if True:
        x = yield 3
    else:
        x = yield 5


from typing import TYPE_CHECKING

# OK
if TYPE_CHECKING:
    x = 3
else:
    x = 5

# SIM108 - should suggest
# z = cond or other_cond
if cond:
    z = cond 
else:
    z = other_cond

# SIM108 - should suggest
# z = cond and other_cond
if not cond:
    z = cond
else:
    z = other_cond

# SIM108 - should suggest
# z = not cond and other_cond
if cond:
    z = not cond
else:
    z = other_cond

# SIM108 does not suggest
# a binary option in these cases,
# despite the fact that `bool` 
# is a subclass of both `int` and `float`
# so, e.g. `True == 1`.
# (Of course, these specific expressions 
# should be simplified for other reasons...)
if True:
    z = 1
else:
    z = other

if False:
    z = 1
else:
    z = other

if 1:
    z = True
else:
    z = other

# SIM108 does not suggest a binary option in this
# case, since we'd be reducing the number of calls
# from Two to one.
if foo():
    z = foo()
else:
    z = other

# SIM108 does not suggest a binary option in this
# case, since we'd be reducing the number of calls
# from Two to one.
if foo():
    z = not foo()
else:
    z = other


# These two cases double as tests for f-string quote preservation. The first
# f-string should preserve its double quotes, and the second should preserve
# single quotes
if cond:
    var = "str"
else:
    var = f"{first}-{second}"

if cond:
    var = "str"
else:
    var = f'{first}-{second}'
