letters = ["a", "b", "c"]

for index, letter in enumerate(letters):
    print(letters[index])  # PLR1736
    blah = letters[index]  # PLR1736
    letters[index] = letters[index]  # PLR1736 (on the right hand)
    letters[index] = "d"  # Ok
