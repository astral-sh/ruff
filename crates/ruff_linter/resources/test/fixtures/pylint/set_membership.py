# PLR6201
1 in [1, 2, 3]

# PLR6201
1 in (1, 2, 3)

# PLR6201
def fruit_is_dangerous_for_cat(fruit: str) -> bool:
    return fruit in ["cherry", "grapes"]

# Ok
fruits = ["cherry", "grapes"]
"cherry" in fruits
