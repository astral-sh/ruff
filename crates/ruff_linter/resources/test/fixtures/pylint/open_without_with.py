def foo(f):
    pass


# Correct usage

with open('path', "wb"):
    pass

foo('a')

# Bad usage


open('path', "wb")  # [open-without-with]

f1 = open('path', "wb")  # [open-without-with]

f = open('path', "wb")  # [open-without-with]
f.close()

foo(open('path', "wb"))  # [open-without-with]


with foo(open('path', "wb")) as f:  # [open-without-with]
    pass




