from typing import no_type_check


# Errors

@no_type_check
class C:
    def f(self, arg: "this isn't python") -> "this isn't python either":
        x: "this also isn't python" = 1


# No errors

@no_type_check
def f(arg: "this isn't python") -> "this isn't python either":
    x: "this also isn't python" = 0
