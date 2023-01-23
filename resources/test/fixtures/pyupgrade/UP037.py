import sys
import six

if sys.version_info < (3,):
    print("Python 2")
else:
    print("Python 3")

if six.PY2:
    print("Python 2")
else:
    print("Python 3")

if not six.PY2:
    print("Python 2")
else:
    print("Python 3")
