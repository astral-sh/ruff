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
