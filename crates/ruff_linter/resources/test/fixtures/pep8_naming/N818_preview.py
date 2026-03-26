class RootError(Exception):
    pass


class CustomBase(Exception):
    pass


class Child(CustomBase):
    pass


class GrandChild(Child):
    pass


class ValidationErrr(ValueError):
    pass


class NestedValidation(ValidationErrr):
    pass


class HardExit(BaseException):
    pass
