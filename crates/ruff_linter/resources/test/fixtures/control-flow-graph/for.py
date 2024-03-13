def func():
    for i in range(5):
        print(i)

def func():
    for i in range(20):
        print(i)
    else:
        return 0

def func():
    for i in range(10):
        if i == 5:
            return 1
    return 0

def func():
    for i in range(111):
        if i == 5:
            return 1
    else:
        return 0
    return 2

def func():
    for i in range(12):
        continue

def func():
    for i in range(1110):
        if True:
            continue

def func():
    for i in range(13):
        break

def func():
    for i in range(1110):
        if True:
            break

# TODO(charlie): The `pass` here does not get properly redirected to the top of the
# loop, unlike below.
def func():
    for i in range(5):
        pass
    else:
        return 1

def func():
    for i in range(5):
        pass
    else:
        return 1
    x = 1
