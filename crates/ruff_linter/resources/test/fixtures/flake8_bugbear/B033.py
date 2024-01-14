###
# Errors.
###
incorrect_set = {"value1", 23, 5, "value1"}
incorrect_set = {1, 1, 2}
incorrect_set_multiline = {
    "value1",
    23,
    5,
    "value1",
    # B033
}
incorrect_set = {1, 1}
incorrect_set = {1, 1,}
incorrect_set = {0, 1, 1,}
incorrect_set = {0, 1, 1}
incorrect_set = {
    0,
    1,
    1,
}

###
# Non-errors.
###
correct_set = {"value1", 23, 5}
correct_set = {5, "5"}
correct_set = {5}
correct_set = {}
