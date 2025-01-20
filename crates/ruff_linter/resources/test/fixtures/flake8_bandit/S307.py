import os

print(eval("1+1"))  # S307
print(eval("os.getcwd()"))  # S307


class Class(object):
    def eval(self):
        print("hi")

    def foo(self):
        self.eval()  # OK


# https://github.com/astral-sh/ruff/issues/15522
map(eval, [])
foo = eval
