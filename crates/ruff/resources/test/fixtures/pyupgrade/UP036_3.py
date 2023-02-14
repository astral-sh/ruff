import sys

if            sys.version_info < (3,0):
    print("py2")
    for item in range(10):
        print(f"PY2-{item}")
else                                :
    print("py3")
    for item in range(10):
        print(f"PY3-{item}")

if False:
    if            sys.version_info < (3,0):
        print("py2")
        for item in range(10):
            print(f"PY2-{item}")
    else                                :
        print("py3")
        for item in range(10):
            print(f"PY3-{item}")


if            sys.version_info < (3,0): print("PY2!")
else                               : print("PY3!")
