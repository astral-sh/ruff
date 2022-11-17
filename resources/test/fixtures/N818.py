class Error(Exception):
    pass


class AnotherError(Exception):
    pass


class C(Exception):
    pass


class D(AnotherError):
    pass
