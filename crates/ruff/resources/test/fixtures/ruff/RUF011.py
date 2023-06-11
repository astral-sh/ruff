data = ["some", "Data"]

# Ok
{value: value.upper() for value in data}
{value.lower(): value.upper() for value in data}
{v: v*v for v in range(10)}
{(0, "a", v): v*v for v in range(10)} # Tuple with variable

# Errors
{"key": value.upper() for value in data}
{True: value.upper() for value in data}
{0: value.upper() for value in data}
{(1, "a"): value.upper() for value in data} # constant tuple
