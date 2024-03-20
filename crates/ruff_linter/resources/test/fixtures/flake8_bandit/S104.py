def func(address):
    print(address)


# OK
"OK"

# Error
"0.0.0.0"
'0.0.0.0'
f"0.0.0.0"


# Error
func("0.0.0.0")


def my_func():
    x = "0.0.0.0"
    print(x)


# Implicit string concatenation
"0.0.0.0" f"0.0.0.0{expr}0.0.0.0"
