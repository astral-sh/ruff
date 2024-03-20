def  test(<RANGE_START>a, b, c:    str<RANGE_END>, d):
    print ( "Don't format the body when only making changes to the clause header")


print( "Should not get formatted")


class <RANGE_START>  Test(OtherClass<RANGE_END>)\
    : # comment

    # Should not get formatted
    def __init__( self):
        print("hello")

print( "don't format this")


def  test2(<RANGE_START>a, b, c:    str, d):<RANGE_END>
    print ( "Don't format the body when only making changes to the clause header")


print( "Should not get formatted")


def  test3(<RANGE_START>a, b, c:    str, d):<RANGE_END> # fmt: skip
    print ( "Don't format the body when only making changes to the clause header")



def  test4(<RANGE_START>   a):
    print("Format this"  )

    if True:
        print( "and this")<RANGE_END>

    print("Not this" )


<RANGE_START>if a   + b :              # trailing clause header comment<RANGE_END>
    print("Not formatted"  )


<RANGE_START>if b   + c :<RANGE_END>              # trailing clause header comment
    print("Not formatted"  )
