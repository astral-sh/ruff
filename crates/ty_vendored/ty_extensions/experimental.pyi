# ruff: file-ignore[docstring-in-stub]

class Refines[After, Returned]:
    """Experimental method return annotation describing a receiver transition.

    After normal completion, the receiver has type `After` and the call returns
    `Returned`. Intersections in `After` must be written explicitly.
    """
