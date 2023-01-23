import sys
import six

if sys.version_info < (3,):
    print("Python 2")
elif sys.version_info < (3,5):
    print("Python 3.4")
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
