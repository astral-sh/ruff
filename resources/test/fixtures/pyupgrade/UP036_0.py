import sys

if sys.version_info < (3,0):
    print("py2")
else:
    print("py3")

if sys.version_info < (3,0):
    if True:
        print("py2!")
    else:
        print("???")
else:
    print("py3")

if sys.version_info < (3,0): print("PY2!")
else: print("PY3!")

if True:
    if sys.version_info < (3,0):
        print("PY2")
    else:
        print("PY3")

if sys.version_info < (3,0): print(1 if True else 3)
else:
    print("py3")

if sys.version_info < (3,0):
    def f():
        print("py2")
else:
    def f():
        print("py3")
        print("This the next")

if sys.version_info > (3,0):
    print("py3")
else:
    print("py2")


x = 1

if sys.version_info > (3,0):
    print("py3")
else:
    print("py2")
    # ohai

x = 1

if sys.version_info > (3,0): print("py3")
else: print("py2")

if sys.version_info > (3,):
    print("py3")
else:
    print("py2")

if True:
    if sys.version_info > (3,):
        print("py3")
    else:
        print("py2")

if sys.version_info < (3,):
    print("py2")
else:
    print("py3")

def f():
    if sys.version_info < (3,0):
        try:
            yield
        finally:
            pass
    else:
        yield


class C:
    def g():
        pass

    if sys.version_info < (3,0):
        def f(py2):
            pass
    else:
        def f(py3):
            pass

    def h():
        pass

if True:
    if sys.version_info < (3,0):
        2
    else:
        3

    # comment

if sys.version_info < (3,0):
    def f():
        print("py2")
    def g():
        print("py2")
else:
    def f():
        print("py3")
    def g():
        print("py3")

if True:
    if sys.version_info > (3,):
        print(3)
    # comment
    print(2+3)

if True:
    if sys.version_info > (3,): print(3)

if True:
    if sys.version_info > (3,):
        print(3)


if True:
    if sys.version_info <= (3, 0):
        expected_error = []
    else:
        expected_error = [
"<stdin>:1:5: Generator expression must be parenthesized",
"max(1 for i in range(10), key=lambda x: x+1)",
"    ^",
        ]


if sys.version_info <= (3, 0):
    expected_error = []
else:
    expected_error = [
"<stdin>:1:5: Generator expression must be parenthesized",
"max(1 for i in range(10), key=lambda x: x+1)",
"    ^",
    ]


if sys.version_info > (3,0):
    """this
is valid"""

    """the indentation on
    this line is significant"""

    "this is" \
    "allowed too"

    ("so is"
     "this for some reason")

if sys.version_info > (3, 0): expected_error = \
    []

if sys.version_info > (3, 0): expected_error = []

if sys.version_info > (3, 0): \
    expected_error = []

if True:
    if sys.version_info > (3, 0): expected_error = \
    []

if True:
    if sys.version_info > (3, 0): expected_error = []

if True:
    if sys.version_info > (3, 0): \
    expected_error = []
