import typing


# Errors

@typing.no_type_check
class C:
    def f(self, arg: "B") -> "S":
        x: "B" = 1


# No errors

@typing.no_type_check
def f(arg: "A") -> "R":
    x: "A" = 1
