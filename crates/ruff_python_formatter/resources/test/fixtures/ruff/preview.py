"""
Black's `Preview.module_docstring_newlines`
"""
first_stmt_after_module_level_docstring = 1


class CachedRepository:
    # Black's `Preview.dummy_implementations`
    def get_release_info(self): ...


def raw_docstring():

    r"""Black's `Preview.accept_raw_docstrings`
        a
            b
    """
    pass


def reference_docstring_newlines():

    """A regular docstring for comparison
        a
            b
    """
    pass


class RemoveNewlineBeforeClassDocstring:

    """Black's `Preview.no_blank_line_before_class_docstring`"""

