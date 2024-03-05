class MyClass:

    # Range that falls entirely in a suppressed range
    # fmt: off<RANGE_START>
    def method(  self  ):
        print   ( "str" )
    <RANGE_END># fmt: on

    # This should net get formatted because it isn't in a formatting range.
    def not_in_formatting_range ( self): ...


    # Range that starts in a suppressed range and ends in a formatting range
    # fmt: off<RANGE_START>
    def other(  self):
        print   ( "str" )

    # fmt: on

    def formatted  ( self):
        pass
    <RANGE_END>
    def outside_formatting_range (self): pass

