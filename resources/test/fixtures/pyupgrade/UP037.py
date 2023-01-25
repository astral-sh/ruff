import sys
import six

if sys.version_info < (2,0):
    print("This script requires Python 2.0 or greater.")
elif sys.version_info < (2,6):
    print("This script requires Python 2.6 or greater.")
elif sys.version_info < (3,):
    print("This script requires Python 3.0 or greater.")
elif sys.version_info < (3,0):
    print("This script requires Python 3.0 or greater.")
elif sys.version_info < (3,12):
    print("Python 3.0 or 3.1 or 3.2")
else:
    print("Python 3")

"""
if six.PY2:
    print("Python 2")
else:
    print("Python 3")

if not six.PY2:
    print("Python 2")
else:
    print("Python 3")
"""
