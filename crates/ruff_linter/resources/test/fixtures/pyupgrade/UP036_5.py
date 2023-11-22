import sys

if sys.version_info < (3, 8):

    def a():
        if b:
            print(1)
        elif c:
            print(2)
        return None

else:
    pass


import sys

if sys.version_info < (3, 8):
    pass

else:

    def a():
        if b:
            print(1)
        elif c:
            print(2)
        else:
            print(3)
        return None
