# Bad
if a:
    b = c
else:
    b = d

# Good
b = c if a else d

# https://github.com/MartinThoma/flake8-simplify/issues/115
if a:
    b = c
elif c:
    b = a
else:
    b = d

if True:
    pass
elif a:
    b = 1
else:
    b = 2

if True:
    pass
else:
    if a:
        b = 1
    else:
        b = 2

import sys

if sys.version_info >= (3, 9):
    randbytes = random.randbytes
else:
    randbytes = _get_random_bytes

if sys.platform == "darwin":
    randbytes = random.randbytes
else:
    randbytes = _get_random_bytes

if sys.platform.startswith("linux"):
    randbytes = random.randbytes
else:
    randbytes = _get_random_bytes
