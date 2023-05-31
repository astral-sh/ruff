"""Test iterate-over-set rule.

Should trigger three times.
"""

# True positives.

for item in {"apples", "lemons", "water"}:  # flags in-line set literals
    print(f"I like {item}.")

for item in set("apples", "lemons", "water"):  # flags set() calls
    print(f"I like {item}.")

for number in {i for i in range(10)}:  # flags set comprehensions
    print(number)

# False negatives.

numbers = [i for i in {1, 2, 3}]  # rule not checked for comprehensions (yet)

# True negatives.

items = {"apples", "lemons", "water"}
for item in items:  # only complains about in-line sets (as per Pylint)
    print(f"I like {item}.")

for item in ["apples", "lemons", "water"]:  # lists are fine
    print(f"I like {item}.")

for item in ("apples", "lemons", "water"):  # tuples are fine
    print(f"I like {item}.")
