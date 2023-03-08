x = "a string"
y = "another string"

if x is "" or x == "":
    print("x is an empty string")

if y is not "" or y != "":
    print("y is not an empty string")

if x and not y:
    print("x is not an empty string, but y is an empty string")
