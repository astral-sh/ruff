x = "a string"
y = "another string"
z = ""


def errors():
    if x is "" or x == "":
        print("x is an empty string")

    if y is not "" or y != "":
        print("y is not an empty string")

    if "" != z:
        print("z is an empty string")


def ok():
    if x and not y:
        print("x is not an empty string, but y is an empty string")


data.loc[data["a"] != ""]
data.loc[data["a"] != "", :]
