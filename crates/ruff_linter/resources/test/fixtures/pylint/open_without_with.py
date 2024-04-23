from pathlib import Path


def foo(f):
    pass


# Correct usage
with open('path', "wb"):
    pass

with open('path', "wb") as f:
    pass

with open('path', "wb") as f:
    foo(f)

foo('a')

with Path('path').open("wb"):
    pass

with Path('path').open("wb") as f:
    pass




# Bad usage
open()  # [open-without-with]

open('path', "wb")  # [open-without-with]

f = open('path', "wb")  # [open-without-with]

f.close()

foo(open('path', "wb"))  # [open-without-with]

with foo(open('path', "wb")) as f:  # [open-without-with]
    pass

Path('path').open()  # [open-without-with]
