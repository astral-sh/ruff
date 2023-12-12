# The percent-percent comments are Spyder IDE cells.


# %%
def func():
    x = """
    a really long string
    """
    lcomp3 = [
        # This one is actually too long to fit in a single line.
        element.split("\n", 1)[0]
        # yup
        for element in collection.select_elements()
        # right
        if element is not None
    ]
    # Capture each of the exceptions in the MultiError along with each of their causes and contexts
    if isinstance(exc_value, MultiError):
        embedded = []
        for exc in exc_value.exceptions:
            if exc not in _seen:
                embedded.append(
                    # This should be left alone (before)
                    traceback.TracebackException.from_exception(
                        exc,
                        limit=limit,
                        lookup_lines=lookup_lines,
                        capture_locals=capture_locals,
                        # copy the set of _seen exceptions so that duplicates
                        # shared between sub-exceptions are not omitted
                        _seen=set(_seen),
                    )
                    # This should be left alone (after)
                )

    # everything is fine if the expression isn't nested
    traceback.TracebackException.from_exception(
        exc,
        limit=limit,
        lookup_lines=lookup_lines,
        capture_locals=capture_locals,
        # copy the set of _seen exceptions so that duplicates
        # shared between sub-exceptions are not omitted
        _seen=set(_seen),
    )


# %%
