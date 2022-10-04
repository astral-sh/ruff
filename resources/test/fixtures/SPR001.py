class Parent:
    def method(self):
        pass

    def wrong(self):
        pass


class Child(Parent):
    def method(self):
        parent = super()  # ok
        super().method()  # ok
        Parent.method(self)  # ok
        Parent.super(1, 2)  # ok

    def wrong(self):
        parent = super(Child, self)  # wrong
        super(Child, self).method  # wrong
        super(
            Child,
            self,
        ).method()  # wrong


class BaseClass:
    def f(self):
        print("f")


def defined_outside(self):
    super(MyClass, self).f()  # CANNOT use super()


class MyClass(BaseClass):
    def normal(self):
        super(MyClass, self).f()  # can use super()
        super().f()

    def different_argument(self, other):
        super(MyClass, other).f()  # CANNOT use super()

    def comprehension_scope(self):
        [super(MyClass, self).f() for x in [1]]  # CANNOT use super()

    def inner_functions(self):
        def outer_argument():
            super(MyClass, self).f()  # CANNOT use super()

        def inner_argument(self):
            super(MyClass, self).f()  # can use super()
            super().f()

        outer_argument()
        inner_argument(self)

    def inner_class(self):
        class InnerClass:
            super(MyClass, self).f()  # CANNOT use super()

            def method(inner_self):
                super(MyClass, self).f()  # CANNOT use super()

        InnerClass().method()

    defined_outside = defined_outside
