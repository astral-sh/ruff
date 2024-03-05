def test ():
    print( "hello" )
    <RANGE_START># leading comment
    1  + 2

    print( "world" )<RANGE_END>

    print( "unformatted" )


print( "Hy" )


def test2 ():
    print( "hello" )
    # Leading comments don't get formatted. That's why Ruff won't fixup
    # the indentation here. That's something we might want to explore in the future
    # leading comment 1<RANGE_START>
        # leading comment 2
    1  + 2

    print( "world" )<RANGE_END>

    print( "unformatted" )

def test3 ():
    <RANGE_START>print( "hello" )
    # leading comment 1
        # leading comment 2
    1  + 2

    print( "world" )<RANGE_END>

    print( "unformatted" )
