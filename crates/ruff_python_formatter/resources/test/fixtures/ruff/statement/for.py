for x in y: # trailing test comment
    pass # trailing last statement comment

    # trailing for body comment

# leading else comment

else: # trailing else comment
    pass

    # trailing else body comment


for aVeryLongNameThatSpillsOverToTheNextLineBecauseItIsExtremelyLongAndGoesOnAndOnAndOnAndOnAndOnAndOnAndOnAndOnAndOn in anotherVeryLongNameThatSpillsOverToTheNextLineBecauseItIsExtremelyLongAndGoesOnAndOnAndOnAndOnAndOnAndOnAndOnAndOnAndOn: # trailing comment
    pass

else:
    ...

for (
    x,
    y,
    ) in z: # comment
    ...


# remove brackets around x,y but keep them around z,w
for (x, y) in (z, w):
    ...


# type comment
for x in (): # type: int
    ...

# Tuple parentheses for iterable.
for x in 1, 2, 3:
    pass

for x in (1, 2, 3):
    pass

for x in 1, 2, 3,:
    pass

# Don't keep parentheses around right target if it can made fit by breaking sub expressions
for column_name, (
    referenced_column_name,
    referenced_table_name,
) in relations.items():
    pass

for column_name, [
    referenced_column_name,
    referenced_table_name,
] in relations.items():
    pass

for column_name, [
    referenced_column_name,
    referenced_table_name,
], in relations.items():
    pass

for (
    # leading comment
    column_name, [
    referenced_column_name,
    referenced_table_name,
]) in relations.items():
    pass

