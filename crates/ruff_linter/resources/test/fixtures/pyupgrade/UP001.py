class A:
    __metaclass__ = type


class B:
    __metaclass__ = type

    def __init__(self) -> None:
        pass


class C(metaclass=type):
    pass
