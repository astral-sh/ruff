input = [1, 2, 3]
otherInput = [2, 3, 4]

# Don't trigger
zip(input, otherInput)
list(zip(input, otherInput))

# Error - prefer pairwise here
zip(input, input[1:])
list(zip(input, input[1:]))

# Don't want the error triggered here since it's not successive - pairwise() is not a valid substitute!
zip(input, input[2:])
