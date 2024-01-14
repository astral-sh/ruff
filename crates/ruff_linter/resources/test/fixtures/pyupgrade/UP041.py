import asyncio, socket
# These should be fixed
try:
    pass
except asyncio.TimeoutError:
    pass

try:
    pass
except socket.timeout:
    pass

# Should NOT be in parentheses when replaced

try:
    pass
except (asyncio.TimeoutError,):
    pass

try:
    pass
except (socket.timeout,):
    pass

try:
    pass
except (asyncio.TimeoutError, socket.timeout,):
    pass

# Should be kept in parentheses (because multiple)

try:
    pass
except (asyncio.TimeoutError, socket.timeout, KeyError, TimeoutError):
    pass

# First should change, second should not

from .mmap import error
try:
    pass
except (asyncio.TimeoutError, error):
    pass

# These should not change

from foo import error

try:
    pass
except (TimeoutError, error):
    pass

try:
    pass
except:
    pass

try:
    pass
except TimeoutError:
    pass


try:
    pass
except (TimeoutError, KeyError):
    pass
