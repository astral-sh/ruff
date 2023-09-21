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
