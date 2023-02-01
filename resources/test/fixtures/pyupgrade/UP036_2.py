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

# OK
if sys.version_info < (3,0):
    pass

if True:
    if sys.version_info < (3,0):
        pass

if sys.version_info < (3,0):
    pass
elif False:
    pass

if sys.version_info > (3,):
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

if sys.version_info > (3, 15):
    3-15
else:
    3-16

if sys.version_info < (3, 15):
    3-16
else:
    3-15

if sys.version_info >= (3, 15):
    3-15
else:
    3-16

if sys.version_info <= (3, 15):
    3-16
else:
    3-15
