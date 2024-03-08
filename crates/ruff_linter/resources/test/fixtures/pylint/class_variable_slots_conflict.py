class Ok:  # OK
    __slots__ = ("_a", "b")

    def __init__(self, a, b):
        self._a = a
        self.b = b

    @property
    def a(self):
        return self._a

    def c(self):
        print("c")


class B:
    __slots__ = ("a", "b", "c")
    b = None

    def __init__(self, a, b):
        self.a = a
        self.b = b

    @property
    def a(self):
        return self.a

    def c(self):
        print("c") 
        
        
class CheckList:  # Not ok
    __slots__ = ["a", "b", "c"]
    a = None
    b, c = 1, 2  # these aren't detected properly


class CheckSet:
    __slots__ = {"a", "b", "c"}
    a = None
    b, c = 1, 2    
    
    
class CheckDict:
    __slots__ = {"a": 0, "b": 1, "c": 2}
    a = None
    b, c = None, None


items = ["a"]  # TODO


class NotOk:
    __slots__ = list(items)
    a = None


class CheckSomethingElse:
    __slots__ = (*["a"],)
    age = None


class CheckIf:
    __slots__ = ["a"] if True else []
    a = None


class CheckStaticMethod:
    __slots__ = ("a")
    
    @staticmethod
    def a():  # OK
        pass


class CheckOtherUnpack:
    __slots__, age = (("a",), 0)  # TODO
