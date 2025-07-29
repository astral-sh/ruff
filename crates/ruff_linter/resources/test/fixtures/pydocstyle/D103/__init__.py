from .markers import no_docs_needed


def must_have_docs():  # lint error
    pass


@no_docs_needed
def ok_to_have_no_docs():  # no lint
    pass
