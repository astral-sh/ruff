"""
Should emit:
B027 - on lines 13, 16, 19, 23
"""
from abc import ABC


class AbstractClass(ABC):
    def empty_1(self):  # error
        ...

    def empty_2(self):  # error
        pass

    def body_1(self):
        print("foo")
        ...

    def body_2(self):
        self.body_1()


def foo():
    class InnerAbstractClass(ABC):
        def empty_1(self):  # error
            ...

        def empty_2(self):  # error
            pass

        def body_1(self):
            print("foo")
            ...

        def body_2(self):
            self.body_1()

    return InnerAbstractClass

