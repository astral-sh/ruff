import sys
from sys import version_info

if sys.version_info > (3, 5):
    3+6
else:
    3-5

if version_info > (3, 5):
    3+6
else:
    3-5

if sys.version_info >= (3,6):
    3+6
else:
    3-5

if version_info >= (3,6):
    3+6
else:
    3-5

if sys.version_info < (3,6):
    3-5
else:
    3+6

if sys.version_info <= (3,5):
    3-5
else:
    3+6

if sys.version_info <= (3, 5):
    3-5
else:
    3+6

if sys.version_info >= (3, 5):
    pass

# These below tests should NOT change
if six.PY2:
    pass

if True:
    if six.PY2:
        pass

if six.PY2:
    pass
elif False:
    pass

if six.PY3:
    pass
elif False:
    pass

if sys.version_info[0] > "2":
    3
else:
    2

from .sys import version_info
if version_info < (3,):
    print(2)
else:
    print(3)
