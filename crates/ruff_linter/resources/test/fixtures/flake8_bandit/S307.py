import os

print(eval("1+1"))  # S307
print(eval("os.getcwd()"))  # S307


class Class(object):
    def eval(self):
        print("hi")

    def foo(self):
        self.eval()  # OK
