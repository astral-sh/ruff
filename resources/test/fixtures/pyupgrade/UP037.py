import sys
import six
from six import PY2, PY3

if six.PY2:
    print("py2")
else:
    print("py3")

if six.PY2:
    if True:
        print("py2!")
    else:
        print("???")
else:
    print("py3")

if six.PY2: print("PY2!")
else: print("PY3!")

if True:
    if six.PY2:
        print("PY2")
    else:
        print("PY3")

if six.PY2: print(1 if True else 3)
else:
    print("py3")

if six.PY2:
    def f():
        print("py2")
else:
    def f():
        print("py3")
        print("This the next")

if not six.PY2:
    print("py3")
else:
    print("py2")


x = 1

if not six.PY2:
    print("py3")
else:
    print("py2")
    # ohai

x = 1

if not six.PY2: print("py3")
else: print("py2")

if six.PY3:
    print("py3")
else:
    print("py2")

if True:
    if six.PY3:
        print("py3")
    else:
        print("py2")

if not PY3:
    print("py2")
else:
    print("py3")

def f():
    if six.PY2:
        try:
            yield
        finally:
            pass
    else:
        yield


class C:
    def g():
        pass

    if six.PY2:
        def f(py2):
            pass
    else:
        def f(py3):
            pass

    def h():
        pass

if True:
    if six.PY2:
        2
    else:
        3

    # comment

if six.PY2:
    def f():
        print("py2")
    def g():
        print("py2")
else:
    def f():
        print("py3")
    def g():
        print("py3")
