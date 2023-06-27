def func():
    while False:
        return "unreachable"
    return 1

def func():
    while False:
        return "unreachable"
    else:
        return 1

def func():
    while False:
        return "unreachable"
    else:
        return 1
    return "also unreachable"

def func():
    while True:
        return 1
    return "unreachable"

def func():
    while True:
        return 1
    else:
        return "unreachable"

def func():
    while True:
        return 1
    else:
        return "unreachable"
    return "also unreachable"

def func():
    i = 0
    while False:
        i += 1
    return i

def func():
    i = 0
    while True:
        i += 1
    return i

def func():
    while True:
        pass
    return 1

def func():
    i = 0
    while True:
        if True:
            print("ok")
        i += 1
    return i

def func():
    i = 0
    while True:
        if False:
            print("ok")
        i += 1
    return i

def func():
    while True:
        if True:
            return 1
    return 0

def func():
    while True:
        continue

def func():
    while False:
        continue

def func():
    while True:
        break

def func():
    while False:
        break

def func():
    while True:
        if True:
            continue

def func():
    while True:
        if True:
            break
