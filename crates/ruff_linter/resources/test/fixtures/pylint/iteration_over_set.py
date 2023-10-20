# Errors

for item in {1}:
    print(f"I can count to {item}!")

for item in {"apples", "lemons", "water"}:  # flags in-line set literals
    print(f"I like {item}.")

for item in {1,}:
    print(f"I can count to {item}!")

for item in {
    "apples", "lemons", "water"
}:  # flags in-line set literals
    print(f"I like {item}.")

numbers_list = [i for i in {1, 2, 3}]  # flags sets in list comprehensions

numbers_set = {i for i in {1, 2, 3}}  # flags sets in set comprehensions

numbers_dict = {str(i): i for i in {1, 2, 3}}  # flags sets in dict comprehensions

numbers_gen = (i for i in {1, 2, 3})  # flags sets in generator expressions

# Non-errors

items = {"apples", "lemons", "water"}
for item in items:  # only complains about in-line sets (as per Pylint)
    print(f"I like {item}.")

for item in ["apples", "lemons", "water"]:  # lists are fine
    print(f"I like {item}.")

for item in ("apples", "lemons", "water"):  # tuples are fine
    print(f"I like {item}.")

numbers_list = [i for i in [1, 2, 3]]  # lists in comprehensions are fine

numbers_set = {i for i in (1, 2, 3)}  # tuples in comprehensions are fine

numbers_dict = {str(i): i for i in [1, 2, 3]}  # lists in dict comprehensions are fine

numbers_gen = (i for i in (1, 2, 3))  # tuples in generator expressions are fine

for item in set(("apples", "lemons", "water")):  # set constructor is fine
    print(f"I like {item}.")

for number in {i for i in range(10)}:  # set comprehensions are fine
    print(number)

for item in {*numbers_set, 4, 5, 6}:  # set unpacking is fine
    print(f"I like {item}.")
