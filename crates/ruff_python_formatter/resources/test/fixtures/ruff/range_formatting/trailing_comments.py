def test1 ():
    print("hello" )

    <RANGE_START>1  + 2<RANGE_END> # trailing comment
    print ("world" )

def test2 ():
    print("hello" )
    # FIXME: For some reason the trailing comment here gets not formatted
    # but is correctly formatted above
    <RANGE_START>1  + 2 # trailing comment<RANGE_END>
    print ("world" )

def test3 ():
    print("hellO" )

    <RANGE_START>1  + 2 # trailing comment

    # trailing section comment
    <RANGE_END>

def test3 ():
    print("hellO" )

    <RANGE_START>1  + 2 # trailing comment
    print("more" ) # trailing comment 2
    # trailing section comment
    <RANGE_END>

print( "world" )
