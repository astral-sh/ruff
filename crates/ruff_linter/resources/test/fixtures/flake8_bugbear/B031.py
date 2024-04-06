import itertools
from itertools import groupby

shoppers = ["Jane", "Joe", "Sarah"]
items = [
    ("lettuce", "greens"),
    ("tomatoes", "greens"),
    ("cucumber", "greens"),
    ("chicken breast", "meats & fish"),
    ("salmon", "meats & fish"),
    ("ice cream", "frozen items"),
]

carts = {shopper: [] for shopper in shoppers}


def collect_shop_items(shopper, items):
    # Imagine this an expensive database query or calculation that is
    # advantageous to batch.
    carts[shopper] += items


# Invoking the `groupby` function directly
for _section, section_items in groupby(items, key=lambda p: p[1]):
    for shopper in shoppers:
        shopper = shopper.title()
        collect_shop_items(shopper, section_items)  # B031
    # We're outside the nested loop and used the group again.
    collect_shop_items(shopper, section_items)  # B031

for _section, section_items in groupby(items, key=lambda p: p[1]):
    collect_shop_items("Jane", section_items)
    collect_shop_items("Joe", section_items)  # B031


# Make sure to detect in other loop constructs as well - `while` loop
for _section, section_items in groupby(items, key=lambda p: p[1]):
    countdown = 3
    while countdown > 0:
        collect_shop_items(shopper, section_items)  # B031
        countdown -= 1

# Make sure to detect in other loop constructs as well - `list` comprehension
collection = []
for _section, section_items in groupby(items, key=lambda p: p[1]):
    collection.append([list(section_items) for _ in range(3)])  # B031

unique_items = set()
another_set = set()
for _section, section_items in groupby(items, key=lambda p: p[1]):
    # For nested loops, it should not flag the usage of the name
    for item in section_items:
        unique_items.add(item)

    # But it should be detected when used again
    for item in section_items:  # B031
        another_set.add(item)

for _section, section_items in groupby(items, key=lambda p: p[1]):
    # Variable has been overridden, skip checking
    section_items = list(unique_items)
    collect_shop_items("Jane", section_items)
    collect_shop_items("Jane", section_items)

for _section, section_items in groupby(items, key=lambda p: p[1]):
    # Variable has been overridden, skip checking
    # Not a realistic situation, just for testing purpose
    (section_items := list(unique_items))
    collect_shop_items("Jane", section_items)
    collect_shop_items("Jane", section_items)

for _section, section_items in groupby(items, key=lambda p: p[1]):
    # This is ok
    collect_shop_items("Jane", section_items)

# Invocation via the `itertools` module
for _section, section_items in itertools.groupby(items, key=lambda p: p[1]):
    for shopper in shoppers:
        collect_shop_items(shopper, section_items)  # B031

for _section, section_items in itertools.groupby(items, key=lambda p: p[1]):
    _ = [collect_shop_items(shopper, section_items) for shopper in shoppers]  # B031

for _section, section_items in itertools.groupby(items, key=lambda p: p[1]):
    # The variable is overridden, skip checking.
    _ = [_ for section_items in range(3)]
    _ = [collect_shop_items(shopper, section_items) for shopper in shoppers]

for _section, section_items in itertools.groupby(items, key=lambda p: p[1]):
    _ = [item for item in section_items]

for _section, section_items in itertools.groupby(items, key=lambda p: p[1]):
    # The iterator is being used for the second time.
    _ = [(item1, item2) for item1 in section_items for item2 in section_items]  # B031

for _section, section_items in itertools.groupby(items, key=lambda p: p[1]):
    if _section == "greens":
        collect_shop_items(shopper, section_items)
    else:
        collect_shop_items(shopper, section_items)
        collect_shop_items(shopper, section_items)  # B031

for _section, section_items in itertools.groupby(items, key=lambda p: p[1]):
    # Mutually exclusive branches shouldn't trigger the warning
    if _section == "greens":
        collect_shop_items(shopper, section_items)
        if _section == "greens":
            collect_shop_items(shopper, section_items)  # B031
        elif _section == "frozen items":
            collect_shop_items(shopper, section_items)  # B031
        else:
            collect_shop_items(shopper, section_items)  # B031
        collect_shop_items(shopper, section_items)  # B031
    elif _section == "frozen items":
        # Mix `match` and `if` statements
        match shopper:
            case "Jane":
                collect_shop_items(shopper, section_items)
                if _section == "fourth":
                    collect_shop_items(shopper, section_items)  # B031
            case _:
                collect_shop_items(shopper, section_items)
    else:
        collect_shop_items(shopper, section_items)
    # Now, it should detect
    collect_shop_items(shopper, section_items)  # B031

for _section, section_items in itertools.groupby(items, key=lambda p: p[1]):
    # Mutually exclusive branches shouldn't trigger the warning
    match _section:
        case "greens":
            collect_shop_items(shopper, section_items)
            match shopper:
                case "Jane":
                    collect_shop_items(shopper, section_items)  # B031
                case _:
                    collect_shop_items(shopper, section_items)  # B031
        case "frozen items":
            collect_shop_items(shopper, section_items)
            collect_shop_items(shopper, section_items)  # B031
        case _:
            collect_shop_items(shopper, section_items)
    # Now, it should detect
    collect_shop_items(shopper, section_items)  # B031

for group in groupby(items, key=lambda p: p[1]):
    # This is bad, but not detected currently
    collect_shop_items("Jane", group[1])
    collect_shop_items("Joe", group[1])


# https://github.com/astral-sh/ruff/issues/4050
for _section, section_items in itertools.groupby(items, key=lambda p: p[1]):
    if _section == "greens":
        for item in section_items:
            collect_shop_items(shopper, item)
    elif _section == "frozen items":
        _ = [item for item in section_items]
    else:
        collect_shop_items(shopper, section_items)

#  Make sure we ignore - but don't fail on more complicated invocations
for _key, (_value1, _value2) in groupby(
    [("a", (1, 2)), ("b", (3, 4)), ("a", (5, 6))], key=lambda p: p[1]
):
    collect_shop_items("Jane", group[1])
    collect_shop_items("Joe", group[1])

#  Make sure we ignore - but don't fail on more complicated invocations
for (_key1, _key2), (_value1, _value2) in groupby(
    [(("a", "a"), (1, 2)), (("b", "b"), (3, 4)), (("a", "a"), (5, 6))],
    key=lambda p: p[1],
):
    collect_shop_items("Jane", group[1])
    collect_shop_items("Joe", group[1])

# Shouldn't trigger the warning when there is a continue, break statement.
for _section, section_items in groupby(items, key=lambda p: p[1]):
    if _section == "greens":
        collect_shop_items(shopper, section_items)
        continue
    elif _section == "frozen items":
        collect_shop_items(shopper, section_items)
        break
    collect_shop_items(shopper, section_items)

# Shouldn't trigger the warning when there is a return statement.
for _section, section_items in groupby(items, key=lambda p: p[1]):
    if _section == "greens":
        collect_shop_items(shopper, section_items)
        return
    elif _section == "frozen items":
        return section_items
    collect_shop_items(shopper, section_items)

# Should trigger the warning for duplicate access, even if is a return statement after.
for _section, section_items in groupby(items, key=lambda p: p[1]):
    if _section == "greens":
        collect_shop_items(shopper, section_items)
        collect_shop_items(shopper, section_items)
        return

# Should trigger the warning for duplicate access, even if is a return in another branch.
for _section, section_items in groupby(items, key=lambda p: p[1]):
    if _section == "greens":
        collect_shop_items(shopper, section_items)
        return
    elif _section == "frozen items":
        collect_shop_items(shopper, section_items)
        collect_shop_items(shopper, section_items)

# Should trigger, since only one branch has a return statement.
for _section, section_items in groupby(items, key=lambda p: p[1]):
    if _section == "greens":
        collect_shop_items(shopper, section_items)
        return
    elif _section == "frozen items":
        collect_shop_items(shopper, section_items)
    collect_shop_items(shopper, section_items)  # B031

# Let's redefine the `groupby` function to make sure we pick up the correct one.
# NOTE: This should always be at the end of the file.
def groupby(data, key=None):
    pass


for name, group in groupby(items):
    collect_shop_items("Jane", items)
    # This shouldn't be flagged as the `groupby` function is different
    collect_shop_items("Joe", items)
