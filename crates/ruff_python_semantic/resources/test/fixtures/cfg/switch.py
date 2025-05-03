def simple_match():
    match bar:
        case This:
            print("this")
        case That:
            print("that")
    print("after")

def simple_if(cond):
    if cond:
        return 1
    return 2

def if_else(cond0,cond1):
    if cond0:
        x = 1
    elif cond1:
        x = 2
    else:
        return 5
