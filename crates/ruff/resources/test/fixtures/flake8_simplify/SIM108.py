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

# OK (false negative)
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
    b = cccccccccccccccccccccccccccccccccccc
else:
    b = ddddddddddddddddddddddddddddddddddddd


# OK (too long)
if True:
    if a:
        b = cccccccccccccccccccccccccccccccccccc
    else:
        b = ddddddddddddddddddddddddddddddddddddd


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
