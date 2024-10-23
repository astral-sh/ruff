def double_quotes_backslash():
    """Sum\\mary."""


def double_quotes_backslash_raw():
    r"""Sum\mary."""


def double_quotes_backslash_uppercase():
    R"""Sum\\mary."""


def shouldnt_add_raw_here():
    "Ruff \U000026a1"


def make_unique_pod_id(pod_id: str) -> str | None:
    r"""
    Generate a unique Pod name.

    Kubernetes pod names must consist of one or more lowercase
    rfc1035/rfc1123 labels separated by '.' with a maximum length of 253
    characters.

    Name must pass the following regex for validation
    ``^[a-z0-9]([-a-z0-9]*[a-z0-9])?(\\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)*$``

    For more details, see:
    https://github.com/kubernetes/kubernetes/blob/release-1.1/docs/design/identifiers.md

    :param pod_id: requested pod name
    :return: ``str`` valid Pod name of appropriate length
    """


def shouldnt_add_raw_here2():
    u"Sum\\mary."


def shouldnt_add_raw_for_double_quote_docstring_contains_docstring():
    """
    This docstring contains another double-quote docstring.

        def foo():
            \"\"\"Foo.\"\"\"
    """


def shouldnt_add_raw_for_double_quote_docstring_contains_docstring2():
    """
    This docstring contains another double-quote docstring.

        def bar():
            \"""Bar.\"""
    
    More content here.
    """


def shouldnt_add_raw_for_single_quote_docstring_contains_docstring():
    '''
    This docstring contains another single-quote docstring.

        def foo():
            \'\'\'Foo.\'\'\'
    
    More content here.
    '''


def shouldnt_add_raw_for_single_quote_docstring_contains_docstring2():
    '''
    This docstring contains another single-quote docstring.

        def bar():
            \'''Bar.\'''
    
    More content here.
    '''

def shouldnt_add_raw_for_docstring_contains_escaped_double_triple_quotes():
    """
    Escaped triple quote \""" or \"\"\".
    """

def shouldnt_add_raw_for_docstring_contains_escaped_single_triple_quotes():
    '''
    Escaped triple quote \''' or \'\'\'.
    '''


def should_add_raw_for_single_double_quote_escape():
    """
    This is single quote escape \".
    """


def should_add_raw_for_single_single_quote_escape():
    '''
    This is single quote escape \'.
    '''
