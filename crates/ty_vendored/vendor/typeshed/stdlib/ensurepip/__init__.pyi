__all__ = ["version", "bootstrap"]

def version() -> str:
    """
    Returns a string specifying the bundled version of pip.
    """

def bootstrap(
    *,
    root: str | None = None,
    upgrade: bool = False,
    user: bool = False,
    altinstall: bool = False,
    default_pip: bool = False,
    verbosity: int = 0,
) -> None:
    """
    Bootstrap pip into the current Python installation (or the given root
    directory).

    Note that calling this function will alter both sys.path and os.environ.
    """
