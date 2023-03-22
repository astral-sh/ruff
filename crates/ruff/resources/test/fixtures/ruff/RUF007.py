input = [1, 2, 3]
otherInput = [2, 3, 4]
foo = [1, 2, 3, 4]

# OK
zip(input, otherInput)  # different inputs
zip(input, otherInput[1:])  # different inputs
zip(input, input[2:])  # not successive
zip(input[:-1], input[2:])  # not successive
list(zip(input, otherInput))  # nested call
zip(input, input[1::2])  # not successive
zip(foo[:-1], foo[1:], foo, strict=False)  # more than 2 inputs
zip(foo[:-1], foo[1:], foo, strict=True)  # more than 2 inputs

# Errors
zip(input, input[1:])
zip(input, input[1::1])
zip(input[:-1], input[1:])
zip(input[1:], input[2:])
zip(input[1:-1], input[2:])
list(zip(input, input[1:]))
list(zip(input[:-1], input[1:]))
zip(foo[:-1], foo[1:], strict=True)
zip(foo[:-1], foo[1:], strict=False)
zip(foo[:-1], foo[1:], strict=bool(foo))
