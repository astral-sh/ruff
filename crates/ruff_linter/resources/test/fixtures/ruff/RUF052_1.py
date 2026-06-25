# Correct usage in loop and comprehension
def process_data():
    return 42
def test_correct_dummy_usage():
    my_list = [{"foo": 1}, {"foo": 2}]

    # Should NOT detect - dummy variable is not used
    [process_data() for _ in my_list]  # OK: `_` is ignored by rule

    # Should NOT detect - dummy variable is not used
    [item["foo"] for item in my_list]  # OK: not a dummy variable name

    # Should NOT detect - dummy variable is not used
    [42 for _unused in my_list]  # OK: `_unused` is not accessed

# Regular For Loops
def test_for_loops():
    my_list = [{"foo": 1}, {"foo": 2}]

    # Should detect used dummy variable
    for _item in my_list:
        print(_item["foo"])  # RUF052: Local dummy variable `_item` is accessed

    # Should detect used dummy variable
    for _index, _value in enumerate(my_list):
        result = _index + _value["foo"]  # RUF052: Both `_index` and `_value` are accessed

# List Comprehensions
def test_list_comprehensions():
    my_list = [{"foo": 1}, {"foo": 2}]

    # Should detect used dummy variable
    result = [_item["foo"] for _item in my_list]  # RUF052: Local dummy variable `_item` is accessed

    # Should detect used dummy variable in nested comprehension
    nested = [[_item["foo"] for _item in _sublist] for _sublist in [my_list, my_list]]
    # RUF052: Both `_item` and `_sublist` are accessed

    # Should detect with conditions
    filtered = [_item["foo"] for _item in my_list if _item["foo"] > 0]
    # RUF052: Local dummy variable `_item` is accessed

# Dict Comprehensions
def test_dict_comprehensions():
    my_list = [{"key": "a", "value": 1}, {"key": "b", "value": 2}]

    # Should detect used dummy variable
    result = {_item["key"]: _item["value"] for _item in my_list}
    # RUF052: Local dummy variable `_item` is accessed

    # Should detect with enumerate
    indexed = {_index: _item["value"] for _index, _item in enumerate(my_list)}
    # RUF052: Both `_index` and `_item` are accessed

    # Should detect in nested dict comprehension
    nested = {_outer: {_inner["key"]: _inner["value"] for _inner in sublist}
              for _outer, sublist in enumerate([my_list])}
    # RUF052: `_outer`, `_inner` are accessed

# Set Comprehensions
def test_set_comprehensions():
    my_list = [{"foo": 1}, {"foo": 2}, {"foo": 1}]  # Note: duplicate values

    # Should detect used dummy variable
    unique_values = {_item["foo"] for _item in my_list}
    # RUF052: Local dummy variable `_item` is accessed

    # Should detect with conditions
    filtered_set = {_item["foo"] for _item in my_list if _item["foo"] > 0}
    # RUF052: Local dummy variable `_item` is accessed

    # Should detect with complex expression
    processed = {_item["foo"] * 2 for _item in my_list}
    # RUF052: Local dummy variable `_item` is accessed

# Generator Expressions
def test_generator_expressions():
    my_list = [{"foo": 1}, {"foo": 2}]

    # Should detect used dummy variable
    gen = (_item["foo"] for _item in my_list)
    # RUF052: Local dummy variable `_item` is accessed

    # Should detect when passed to function
    total = sum(_item["foo"] for _item in my_list)
    # RUF052: Local dummy variable `_item` is accessed

    # Should detect with multiple generators
    pairs = ((_x, _y) for _x in range(3) for _y in range(3) if _x != _y)
    # RUF052: Both `_x` and `_y` are accessed

    # Should detect in nested generator
    nested_gen = (sum(_inner["foo"] for _inner in sublist) for _sublist in [my_list] for sublist in _sublist)
    # RUF052: `_inner` and `_sublist` are accessed

# Complex Examples with Multiple Comprehension Types
def test_mixed_comprehensions():
    data = [{"items": [1, 2, 3]}, {"items": [4, 5, 6]}]

    # Should detect in mixed comprehensions
    result = [
        {_key: [_val * 2 for _val in _record["items"]] for _key in ["doubled"]}
        for _record in data
    ]
    # RUF052: `_key`, `_val`, and `_record` are all accessed

    # Should detect in generator passed to list constructor
    gen_list = list(_item["items"][0] for _item in data)
    # RUF052: Local dummy variable `_item` is accessed
