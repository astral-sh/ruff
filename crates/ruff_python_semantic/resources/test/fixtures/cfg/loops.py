def simple_while():
    while cond:
        foo()

def surround_while():
    x = 1
    while cond:
        x = 2
    x = 2

def nested_while():
    x = 1
    while cond0:
        while cond1:
            while cond2:
                print("hey")
                print("there")
            print("okay")
        print("more")
    print("done")


def never_loop_return():
    while cond:
        return 1
    print("something")
    return 2

def never_loop_break():
    while cond:
        break
    print("never looped")

def nested_loop_break():
    while cond0:
        while cond1:
            print("break to outer")
            break
        break
        
def for_else():
    for i in [1,2,3]:
        continue
    else:
        print("ok")
    return 3    
