def func(address):
    print(address)


# OK
"OK"

# Error
"0.0.0.0"
'0.0.0.0'


# Error
func("0.0.0.0")


def my_func():
    x = "0.0.0.0"
    print(x)
