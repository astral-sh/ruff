class Outer:
    '''
    The outer class.
    '''
    a = 4
    class Inner:
        pass
    __slots__ = ("b",)
