"""Test iterate-over-set rule.

Should trigger three times.
"""

for item in {"apples", "lemons", "water"}:  # iterate-over-set
    print(f"I like {item}.")

for item in ["apples", "lemons", "water"]:
    print(f"I like {item}.")

for item in ("apples", "lemons", "water"):
    print(f"I like {item}.")

items = {"apples", "lemons", "water"}
for item in items:
    print(f"I like {item}.")
