match x:
    case y if a := 1: ...
match x:
    case y if a if True else b: ...
match x:
    case y if lambda a: b: ...
def _():
    match x:
        case y if (yield x): ...
