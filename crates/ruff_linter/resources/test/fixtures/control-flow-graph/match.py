def func(status):
    match status:
        case _:
            return 0
    return "unreachable"

def func(status):
    match status:
        case 1:
            return 1
    return 0

def func(status):
    match status:
        case 1:
            return 1
        case _:
            return 0

def func(status):
    match status:
        case 1 | 2 | 3:
            return 5
    return 6

def func(status):
    match status:
        case 1 | 2 | 3:
            return 5
        case _:
            return 10
    return 0

def func(status):
    match status:
        case 0:
            return 0
        case 1:
            return 1
        case 1:
            return "1 again"
        case _:
            return 3

def func(status):
    i = 0
    match status, i:
        case _, _:
            return 0

def func(status):
    i = 0
    match status, i:
        case _, 0:
            return 0
        case _, 2:
            return 0

def func(point):
    match point:
        case (0, 0):
            print("Origin")
        case _:
            raise ValueError("oops")

def func(point):
    match point:
        case (0, 0):
            print("Origin")
        case (0, y):
            print(f"Y={y}")
        case (x, 0):
            print(f"X={x}")
        case (x, y):
            print(f"X={x}, Y={y}")
        case _:
            raise ValueError("Not a point")

def where_is(point):
    class Point:
        x: int
        y: int

    match point:
        case Point(x=0, y=0):
            print("Origin")
        case Point(x=0, y=y):
            print(f"Y={y}")
        case Point(x=x, y=0):
            print(f"X={x}")
        case Point():
            print("Somewhere else")
        case _:
            print("Not a point")

def func(points):
    match points:
        case []:
            print("No points")
        case [Point(0, 0)]:
            print("The origin")
        case [Point(x, y)]:
            print(f"Single point {x}, {y}")
        case [Point(0, y1), Point(0, y2)]:
            print(f"Two on the Y axis at {y1}, {y2}")
        case _:
            print("Something else")

def func(point):
    match point:
        case Point(x, y) if x == y:
            print(f"Y=X at {x}")
        case Point(x, y):
            print(f"Not on the diagonal")

def func():
    from enum import Enum
    class Color(Enum):
        RED = 'red'
        GREEN = 'green'
        BLUE = 'blue'

    color = Color(input("Enter your choice of 'red', 'blue' or 'green': "))

    match color:
        case Color.RED:
            print("I see red!")
        case Color.GREEN:
            print("Grass is green")
        case Color.BLUE:
            print("I'm feeling the blues :(")


def func(point):
    match point:
        case (0, 0):
            print("Origin")
        case foo:
            raise ValueError("oops")
