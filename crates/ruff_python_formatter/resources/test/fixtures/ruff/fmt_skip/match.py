def http_error(status):
    match status   : # fmt: skip
        case 400   : # fmt: skip
            return "Bad request"
        case 404:
            return "Not found"
        case 418:
            return "I'm a teapot"
        case _:
            return "Something's wrong with the internet"

# point is an (x, y) tuple
match point:
    case (0,   0): # fmt: skip
        print("Origin")
    case (0, y):
        print(f"Y={y}")
    case (x, 0):
        print(f"X={x}")
    case (x, y):
        print(f"X={x}, Y={y}")
    case _:
        raise ValueError("Not a point")

class Point:
    x: int
    y: int

def location(point):
    match point:
        case Point(x=0, y =0  ) : # fmt: skip
            print("Origin is the point's location.")
        case Point(x=0, y=y):
            print(f"Y={y} and the point is on the y-axis.")
        case Point(x=x, y=0):
            print(f"X={x} and the point is on the x-axis.")
        case Point():
            print("The point is located somewhere else on the plane.")
        case _:
            print("Not a point")


match points:
    case []:
        print("No points in the list.")
    case [
        Point(0, 0)
    ]: # fmt: skip
        print("The origin is the only point in the list.")
    case [Point(x, y)]:
        print(f"A single point {x}, {y} is in the list.")
    case [Point(0, y1), Point(0, y2)]:
        print(f"Two points on the Y axis at {y1}, {y2} are in the list.")
    case _:
        print("Something else is found in the list.")


match test_variable:
    case (
            'warning',
            code,
            40
    ): # fmt: skip
        print("A warning has been received.")
    case ('error', code, _):
        print(f"An error {code} occurred.")


match point:
    case Point(x, y) if x   == y:  # fmt: skip
        print(f"The point is located on the diagonal Y=X at {x}.")
    case Point(x, y):
        print(f"Point is not on the diagonal.")
