def test  ():
    # Leading comments before the statements that should be formatted
    # Don't duplicate the comments
    <RANGE_START>print( "format this")<RANGE_END>  # trailing end of line comment
    # here's some trailing comment as well

print("Do not format this" )

def test  ():
    # Leading comments before the statements that should be formatted
    # Don't duplicate the comments
    <RANGE_START>print( "format this")
    # here's some trailing comment as well
<RANGE_END>

print("Do not format this 2" )
