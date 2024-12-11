def  test(<RANGE_START>a ): <RANGE_END>print("body" )


def  test2(  a):    <RANGE_START>print("body"  )<RANGE_END>


def  test3(  a):    <RANGE_START>print("body"  )

print("more"   )<RANGE_END>
print("after"  )


# The if header and the print statement together are longer than 100 characters.
# The print statement should either be wrapped to fit at the end of the if statement, or be converted to a
# suite body
if aaaaaaaaaaaa + bbbbbbbbbbbbbb + cccccccccccccccccc + ddd:   <RANGE_START>print("aaaa long body, should wrap or be indented"  )<RANGE_END>

# This print statement is too long even when indented. It should be wrapped
if aaaaaaaaaaaa + bbbbbbbbbbbbbb + cccccccccccccccccc + ddd: <RANGE_START>print("aaaa long body, should wrap or be indented", "more content to make it exceed the 88 chars limit")<RANGE_END>
