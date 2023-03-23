class A:
    def foo(self):
        return 1

    def bar(self):
        self.foo()  # NEW100
        b = self.foo()  # NEW100
        return self.bar(b)

    def baz(self, val):
        return val


class B:
    def bar(self):
        return 1

    def foo(self):
        return self.bar()  # NEW100

    def baz(self):
        return self.bar()  # NEW100


class C:
    a: A

    def baz(self):
        return self.a.bar()

    def foo(self):
        return foo()


class D:
    def foo(self):
        self.bar()
        return 1

    def bar(self):
        a = self.foo()
        return a


class D1:
    def foo(self):
        a = self.bar()
        return a

    def bar(self):
        a = self.foo()
        return a


class D2:
    def foo(self):
        return self.bar()

    def bar(self):
        return self.foo()


class E:
    def foo(self):
        return self.bar()


def baz():
    return 1


def foo():
    return bar()


def bar():
    return baz()  # NEW100


class F:
    def baz(self):
        return self.foo()

    def foo(self):
        return foo()  # NEW100


class G:
    def baz(self):
        if True:
            return self.foo()

    def foo(self):
        if True:
            return self.baz()


class H:
    def baz(self):
        if True:
            return 1

    def foo(self):
        if True:
            return self.baz()  # NEW100


class J:
    def baz(self):
        if True:
            if True:
                return self.foo()

    def foo(self):
        if True:
            return self.baz()


class A2:
    def foo(self):
        return 1

    def bar(self):
        bop.foo()
        b = bop.foo()
        return self.bar(b)


def fool():
    return 1


class A3:
    def foo():
        def bar():
            x = 1
            return x

        def bop():
            bar()  # NEW100
            baz()
            fool()  # NEW100

        def baz():
            x = 1
            return x
