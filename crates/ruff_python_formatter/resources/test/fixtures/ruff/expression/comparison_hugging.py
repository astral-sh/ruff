selected = [item for item in values if item not in {first, second, third,}]
selected = [item for item in values if item < function(first, second, third,)]
if enabled and value in (first, second, third,) and other_condition:
    pass

# Keep splitting before the comparator if the right-hand header cannot fit.
if very_long_left_hand_name not in very_long_right_hand_function_name():
    pass
if very_long_left_hand_name not in very_long_right_hand_function_name(argument):
    pass

# Keep the layout balanced for bracketed left operands and chained comparisons.
if function(first, second, third,) in {first, second, third,}:
    pass
if first < function(second, third, fourth,) < fifth:
    pass
if (not condition) == function(first, second, third,):
    pass

# Comments must retain a valid position around the operator.
if (
    value  # left-hand comment
    in {first, second, third,}
):
    pass
if value in (  # comment on the right-hand operand
    first, second, third,
):
    pass
