data = ["some", "Data"]
constant = 5

# OK
{value: value.upper() for value in data}
{value.lower(): value.upper() for value in data}
{v: v * v for v in range(10)}
{(0, "a", v): v * v for v in range(10)}  # Tuple with variable
{constant: value.upper() for value in data for constant in data}
{value.attribute: value.upper() for value in data for constant in data}
{constant[value]: value.upper() for value in data for constant in data}
{value[constant]: value.upper() for value in data for constant in data}
{local_id: token for token in tokens if (local_id := _extract_local_id(token)) is not None}
{key: kwargs.get(key) for key in kwargs.keys() if not params.get(key)}

# Errors
{"key": value.upper() for value in data}
{True: value.upper() for value in data}
{0: value.upper() for value in data}
{(1, "a"): value.upper() for value in data}  # Constant tuple
{constant: value.upper() for value in data}
{constant + constant: value.upper() for value in data}
{constant.attribute: value.upper() for value in data}
{constant[0]: value.upper() for value in data}
{tokens: token for token in tokens}

