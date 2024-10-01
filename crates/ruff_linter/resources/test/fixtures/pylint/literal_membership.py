# Errors
1 in [1, 2, 3]
1 in (1, 2, 3)
1 in (
    1, 2, 3
)
fruits = ["cherry", "grapes"]
"cherry" in fruits
_ = {key: value for key, value in {"a": 1, "b": 2}.items() if key in ("a", "b")}

# OK
fruits in [[1, 2, 3], [4, 5, 6]]
fruits in [1, 2, 3]
1 in [[1, 2, 3], [4, 5, 6]]
_ = {key: value for key, value in {"a": 1, "b": 2}.items() if key in (["a", "b"], ["c", "d"])}
