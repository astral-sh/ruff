class C:

    var1 = 1
    var2 = "foo"

    def foo(self):
        self

    foo2 = foo

    def bar(self):
        C.var1
        self.__class__.var2
