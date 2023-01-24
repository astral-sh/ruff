class A:
    def f(self):
        for line in range(10):
            if True:
                pass  # fmt: skip

# output

class A:
    def f(self):
        for line in range(10):
            if True:
                pass  # fmt: skip
