letters = ["a", "b", "c"]


def fix_these():
    for index, letter in enumerate(letters):
        print(letters[index])  # PLR1736
        blah = letters[index]  # PLR1736
        letters[index]: str = letters[index]  # PLR1736 (on the right hand)
        letters[index] += letters[index]  # PLR1736 (on the right hand)
        letters[index] = letters[index]  # PLR1736 (on the right hand)


def dont_fix_these():
    for index, letter in enumerate(letters):
        letters[index] = "d"  # Ok


def value_intentionally_unused():
    for index, _ in enumerate(letters):
        print(letters[index])  # Ok
        blah = letters[index]  # Ok
        letters[index]: str = letters[index]  # Ok
        letters[index] += letters[index]  # Ok
        letters[index] = letters[index]  # Ok
        letters[index] = "d"  # Ok
