match x := 1:
    case _: ...
match (x := 1):
    case _: ...
# Starred expressions are only allowed in tuple expression
match *x | y, z:
    case _: ...
def outer():
    match await x:
        case _: ...
