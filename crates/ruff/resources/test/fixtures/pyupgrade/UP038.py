isinstance(1, (int, float))  # UP038
issubclass("yes", (int, float, str))  # UP038

isinstance(1, int)  # OK
issubclass("yes", int)  # OK
isinstance(1, int | float)  # OK
issubclass("yes", int | str)  # OK
isinstance(1, ())  # OK
isinstance(1, (int, *(str, bytes)))  # OK
