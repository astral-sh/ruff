# Regression test for https://github.com/astral-sh/ruff/pull/20962
# error message:
# `infer_definition_types(Id(1804)): execute: too many cycle iterations`

for name_1 in {
    {{0: name_4 for unique_name_0 in unique_name_1}: 0 for unique_name_2 in unique_name_3 if name_4}: 0
    for unique_name_4 in name_1
    for name_4 in name_1
}:
    pass
