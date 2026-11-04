values = [first_value, second_value] + [third_value, fourth_value]
long_assignment_target_name = [first_value, second_value] + [third_value, fourth_value]
values: list[str] = [first_value, second_value] + [third_value, fourth_value]
values += [first_value, second_value] + [third_value, fourth_value]

def function():
    return [first_value, second_value] + [third_value, fourth_value]

if [first_value, second_value] == [third_value, fourth_value]:
    pass

send([first_value, second_value] + [third_value, fourth_value])
outer = [[first_value, second_value] + [third_value, fourth_value]]
chained = [first_value, second_value] + [third_value, fourth_value] + [fifth_value]
membership = [first_value, second_value] in [third_value, fourth_value]
identity = [first_value, second_value] is [third_value, fourth_value]

magic_left = [first_value, second_value,] + [third_value, fourth_value]
magic_right = [first_value, second_value] + [third_value, fourth_value,]
magic_nested = [[first_value, second_value,]] + [third_value, fourth_value]
singletons = (first_long_value,) + (second_long_value,)
empty = [] + []

long_left = [first_value_with_a_very_long_name, second_value_with_a_very_long_name] + [third_value]
long_right = [first_value] + [second_value_with_a_very_long_name, third_value_with_a_very_long_name]
unicode_values = ["日本語", "日本語", "日本語"] + ["日本語", "日本語", "日本語"]

multiline = ["""first
second"""] + [third_value, fourth_value]
comment_inside = [
    first_value,  # first
    second_value,
] + [third_value, fourth_value]
comment_between = (
    [first_value, second_value]  # first collection
    + [third_value, fourth_value]
)
comment_after = [first_value, second_value] + [third_value, fourth_value]  # both
comment_operator = (
    [first_value, second_value]
    +  # operator
    [third_value, fourth_value]
)
parenthesized_collections = ({first_value, second_value}) == ({third_value, fourth_value})
