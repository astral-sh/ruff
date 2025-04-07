def simple_match():
    match bar:
        case This:
            print("this")
        case That:
            print("that")
    print("after")

def simpel_if(cond):
    if cond:
        return 1
    return 2
