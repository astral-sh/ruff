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


class B:  # Not OK
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
        
        
class CheckList:  # Not OK
    __slots__ = ["a", "b", "c"]
    a = None
    b, c = 1, 2  # these aren't detected properly


class CheckSet:  # Not OK
    __slots__ = {"a", "b", "c"}
    a = None
    b, c = 1, 2    
    
    
class CheckDict:  # Not OK
    __slots__ = {"a": 0, "b": 1, "c": 2}
    a = None
    b, c = None, None


items = ["a"]  # TODO


class NotOk:
    __slots__ = list(items)
    a = None


class CheckUnpackList:  # Not OK
    __slots__ = (*["a", "b"],)
    a = None


class CheckUnpackTuple: # Not OK
    __slots__ = (*("a", "b"),)
    a = None


class CheckIf:  # Not OK
    __slots__ = ["a"] if True else []
    a = None


class CheckTupleWithOneElement:
    __slots__ = "a",
    a = None


class CheckOtherUnpack:  # Not OK
    __slots__, a = (("a",), 0)


class ShouldNotBeOk:  # OK as slots is a string
    __slots__ = "a"
    a = None
