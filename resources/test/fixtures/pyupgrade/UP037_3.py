# This is my place to expiriment with some WILD syntax
import six
import sys

if            six.PY2               :
    print("py2")
    for item in range(10):
        print(f"PY2-{item}")
else                                :
    print("py3")
    for item in range(10):
        print(f"PY3-{item}")

if False:
    if            six.PY2               :
        print("py2")
        for item in range(10):
            print(f"PY2-{item}")
    else                                :
        print("py3")
        for item in range(10):
            print(f"PY3-{item}")


if            six.PY2               : print("PY2!")
else                               : print("PY3!")
