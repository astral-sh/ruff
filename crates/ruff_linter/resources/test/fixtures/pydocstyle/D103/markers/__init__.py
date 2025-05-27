def no_docs_needed(f):
    """Mark function as not needing a docstring."""
    return f


def must_have_docs():  # lint error
    pass


@no_docs_needed
def ok_to_have_no_docs():  # no lint
    pass
