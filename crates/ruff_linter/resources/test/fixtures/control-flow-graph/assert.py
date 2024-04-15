def func():
    assert True

def func():
    assert False

def func():
    assert True, "oops"

def func():
    assert False, "oops"

def func():
    y = 2
    assert y == 2
    assert y > 1
    assert y < 3

def func():
    for i in range(3):
        assert i < x

def func():
    for j in range(3):
        x = 2
    else:
        assert False
    return 1

def func():
    for j in range(3):
        if j == 2:
            print('yay')
            break
    else:
        assert False
    return 1
