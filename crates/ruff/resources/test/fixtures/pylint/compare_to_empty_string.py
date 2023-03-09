x = "a string"
y = "another string"

def should_raise_lint_warning():
    if x is "" or x == "":
        print("x is an empty string")

    if y is not "" or y != "":
        print("y is not an empty string")

def should_not_raise_lint_warning():
    if x and not y:
        print("x is not an empty string, but y is an empty string")
