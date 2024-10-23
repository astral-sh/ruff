def simple():
    """
    First line.

    ```py
    class Abcdefghijklmopqrstuvwxyz(Abc, Def, Ghi, Jkl, Mno, Pqr, Stu, Vwx, Yz, A1, A2, A3, A4, A5):
        def abcdefghijklmnopqrstuvwxyz(self, abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
            def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
                # For 4 space indents, this is just one character shy of
                # tripping the default line width of 88. So it should not be
                # wrapped.
                print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a567)
                return 5
            self.x = doit( 5 )
    ```

    Done.
    """
    pass


# Like simple, but we double everything up to ensure the indent level is
# tracked correctly.
def repeated():
    """
    First line.

    ```py
    class Abcdefghijklmopqrstuvwxyz(Abc, Def, Ghi, Jkl, Mno, Pqr, Stu, Vwx, Yz, A1, A2, A3, A4, A5):
        def abcdefghijklmnopqrstuvwxyz(self, abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
            def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
                # For 4 space indents, this is just one character shy of
                # tripping the default line width of 88. So it should not be
                # wrapped.
                print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a567)
                return 5
            self.x = doit( 5 )

            def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
                # For 4 space indents, this is just one character shy of
                # tripping the default line width of 88. So it should not be
                # wrapped.
                print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a567)
                return 5
            self.x = doit( 5 )

        def abcdefghijklmnopqrstuvwxyz(self, abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
            def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
                # For 4 space indents, this is just one character shy of
                # tripping the default line width of 88. So it should not be
                # wrapped.
                print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a567)
                return 5
            self.x = doit( 5 )

            def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
                # For 4 space indents, this is just one character shy of
                # tripping the default line width of 88. So it should not be
                # wrapped.
                print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a567)
                return 5
            self.x = doit( 5 )


    class Abcdefghijklmopqrstuvwxyz(Abc, Def, Ghi, Jkl, Mno, Pqr, Stu, Vwx, Yz, A1, A2, A3, A4, A5):
        def abcdefghijklmnopqrstuvwxyz(self, abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
            def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
                # For 4 space indents, this is just one character shy of
                # tripping the default line width of 88. So it should not be
                # wrapped.
                print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a567)
                return 5
            self.x = doit( 5 )

            def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
                # For 4 space indents, this is just one character shy of
                # tripping the default line width of 88. So it should not be
                # wrapped.
                print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a567)
                return 5
            self.x = doit( 5 )

        def abcdefghijklmnopqrstuvwxyz(self, abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
            def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
                # For 4 space indents, this is just one character shy of
                # tripping the default line width of 88. So it should not be
                # wrapped.
                print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a567)
                return 5
            self.x = doit( 5 )

            def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
                # For 4 space indents, this is just one character shy of
                # tripping the default line width of 88. So it should not be
                # wrapped.
                print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a567)
                return 5
            self.x = doit( 5 )
    ```

    Done.
    """
    pass


# Like simple, but we make one line exactly one character longer than the limit
# (for 4-space indents) and make sure it gets wrapped.
def barely_exceeds_limit():
    """
    First line.

    ```py
    class Abcdefghijklmopqrstuvwxyz(Abc, Def, Ghi, Jkl, Mno, Pqr, Stu, Vwx, Yz, A1, A2, A3, A4, A5):
        def abcdefghijklmnopqrstuvwxyz(self, abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
            def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
                # For 4 space indents, this is 89 columns, which is one
                # more than the limit. Therefore, it should get wrapped for
                # indent_width >= 4.
                print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a5678)
                return 5
            self.x = doit( 5 )
    ```

    Done.
    """
    pass


# This tests that if the code block is unindented, that it gets indented and
# the dynamic line width setting is applied correctly.
def unindented():
    """
    First line.

```py
class Abcdefghijklmopqrstuvwxyz(Abc, Def, Ghi, Jkl, Mno, Pqr, Stu, Vwx, Yz, A1, A2, A3, A4, A5):
    def abcdefghijklmnopqrstuvwxyz(self, abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
        def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
            # For 4 space indents, this is just one character shy of
            # tripping the default line width of 88. So it should not be
            # wrapped.
            print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a567)
            return 5
        self.x = doit( 5 )
```

    Done.
    """
    pass


# Like unindented, but contains a `print` line where it just barely exceeds the
# globally configured line width *after* its indentation has been corrected.
def unindented_barely_exceeds_limit():
    """
    First line.

```py
class Abcdefghijklmopqrstuvwxyz(Abc, Def, Ghi, Jkl, Mno, Pqr, Stu, Vwx, Yz, A1, A2, A3, A4, A5):
    def abcdefghijklmnopqrstuvwxyz(self, abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
        def abcdefghijklmnopqrstuvwxyz(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4):
            # For 4 space indents, this is 89 columns, which is one
            # more than the limit. Therefore, it should get wrapped for
            # indent_width >= 4.
            print(abc, ddef, ghi, jkl, mno, pqr, stu, vwx, yz, a1, a2, a3, a4, a5678)
            return 5
        self.x = doit( 5 )
```

    Done.
    """
    pass


# See: https://github.com/astral-sh/ruff/issues/9126
def doctest_extra_indent1():
    """
    Docstring example containing a class.

    Examples
    --------
    >>> @pl.api.register_dataframe_namespace("split")
    ... class SplitFrame:
    ...     def __init__(self, df: pl.DataFrame):
    ...         self._df = df
    ...
    ...     def by_first_letter_of_column_values(self, col: str) -> list[pl.DataFrame]:
    ...         return [
    ...             self._df.filter(pl.col(col).str.starts_with(c))
    ...             for c in sorted(
    ...                 set(df.select(pl.col(col).str.slice(0, 1)).to_series())
    ...             )
    ...         ]
    """


# See: https://github.com/astral-sh/ruff/issues/9126
class DoctestExtraIndent2:
    def example2():
        """
        Regular docstring of class method.

        Examples
        --------
        >>> df = pl.DataFrame(
        ...     {"foo": [1, 2, 3], "bar": [6, 7, 8], "ham": ["a", "b", "c"]}
        ... )
        """


# See: https://github.com/astral-sh/ruff/issues/9126
def doctest_extra_indent3():
    """
    Pragma comment.

    Examples
    --------
    >>> af1, af2, af3 = pl.align_frames(
    ...     df1, df2, df3, on="dt"
    ... )  # doctest: +IGNORE_RESULT
    """

# See https://github.com/astral-sh/ruff/issues/13358
def length_doctest():
    """Get the length of the given list of numbers.

    Args:
        numbers: List of numbers.

    Returns:
        Integer length of the list of numbers.

    Example:
        >>> length([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20])
        20
    """


def length_doctest_underindent():
    """Get the length of the given list of numbers.

        Args:
            numbers: List of numbers.

        Returns:
            Integer length of the list of numbers.

    Example:
        >>> length([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20])
        20
    """


# See https://github.com/astral-sh/ruff/issues/13358
def length_markdown():
    """Get the length of the given list of numbers.

    Args:
        numbers: List of numbers.

    Returns:
        Integer length of the list of numbers.

    Example:

        ```
        length([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21])
        ```
    """


# See https://github.com/astral-sh/ruff/issues/13358
def length_rst():
    """
    Do cool stuff::

        length([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21])
    """
    pass


# See https://github.com/astral-sh/ruff/issues/13358
def length_rst_in_section():
    """
    Examples:
        Do cool stuff::

            length([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20])
    """
    pass
