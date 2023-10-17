letters = ["a", "b", "c"]

for index, letter in enumerate(letters):
    print(letters[index])  # PLR1736
    blah = letters[index]  # PLR1736
