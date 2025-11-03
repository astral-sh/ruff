x = 10
def test_1():
    global x  # ok
    x += 1


x = 10
def test_2():
    x += 1      # error
    global x

def test_3():
    print(x)  # error
    global x
    x = 5

def test_4():
    global x
    print(x)
    x = 1

x = 0
test_4()