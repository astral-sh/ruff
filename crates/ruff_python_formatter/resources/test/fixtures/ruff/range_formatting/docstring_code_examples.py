def doctest_simple ():
    <RANGE_START>"""
    Do cool stuff.

    >>> cool_stuff( 1 )
    2
    """
    pass<RANGE_END>


def doctest_only ():
    <RANGE_START>"""
    Do cool stuff.

    >>> def cool_stuff( x ):
    ...     print( f"hi {x}" );
    hi 2
    """<RANGE_END>
    pass


def in_doctest ():
    """
    Do cool stuff.
    <RANGE_START>
    >>> cool_stuff( x )
    >>> cool_stuff( y )
    2<RANGE_END>
    """
    pass

def suppressed_doctest ():
    """
    Do cool stuff.
    <RANGE_START>
    >>> cool_stuff( x )
    >>> cool_stuff( y )
    2<RANGE_END>
    """ # fmt: skip
    pass


def fmt_off_doctest ():
    # fmt: off
    """
    Do cool stuff.
    <RANGE_START>
    >>> cool_stuff( x )
    >>> cool_stuff( y )
    2<RANGE_END>
    """
    # fmt: on
    pass



if True:
    def doctest_long_lines():
        <RANGE_START>
        '''
        This won't get wrapped even though it exceeds our configured
        line width because it doesn't exceed the line width within this
        docstring. e.g, the `f` in `foo` is treated as the first column.
        >>> foo,  bar, quux = this_is_a_long_line(lion, giraffe, hippo, zeba, lemur, penguin, monkey)

        But this one is long enough to get wrapped.
        >>> foo,  bar, quux = this_is_a_long_line(lion, giraffe, hippo, zeba, lemur, penguin, monkey, spider, bear, leopard)

        This one doesn't get wrapped with an indent width of 4 even with `dynamic` line width
        >>> a =  this_is_a_long_line(lion, giraffe, hippo, zebra, lemur, penguin, monkey)

        This one gets wrapped with `dynamic` line width and an indent width of 4 because it exceeds the width by 1
        >>> ab =  this_is_a_long_line(lion, giraffe, hippo, zebra, lemur, penguin, monkey)
        '''
        # This demonstrates a normal line that will get wrapped but won't
        # get wrapped in the docstring above because of how the line-width
        # setting gets reset at the first column in each code snippet.
        foo, bar, quux = this_is_a_long_line(lion, giraffe, hippo, zeba, lemur, penguin, monkey)
        <RANGE_END>


if True:
    def doctest_long_lines():
        <RANGE_START>'''
        This won't get wrapped even though it exceeds our configured
        line width because it doesn't exceed the line width within this
        docstring. e.g, the `f` in `foo` is treated as the first column.
        >>> foo,  bar, quux = this_is_a_long_line(lion, giraffe, hippo, zeba, lemur, penguin, monkey)

        But this one is long enough to get wrapped.
        >>> foo,  bar, quux = this_is_a_long_line(lion, giraffe, hippo, zeba, lemur, penguin, monkey, spider, bear, leopard)

        This one doesn't get wrapped with an indent width of 4 even with `dynamic` line width
        >>> a =  this_is_a_long_line(lion, giraffe, hippo, zebra, lemur, penguin, monkey)

        This one gets wrapped with `dynamic` line width and an indent width of 4 because it exceeds the width by 1
        >>> ab =  this_is_a_long_line(lion, giraffe, hippo, zebra, lemur, penguin, monkey)
        '''<RANGE_END>
        # This demonstrates a normal line that will get wrapped but won't
        # get wrapped in the docstring above because of how the line-width
        # setting gets reset at the first column in each code snippet.
        foo, bar, quux = this_is_a_long_line(lion, giraffe, hippo, zeba, lemur, penguin, monkey)

