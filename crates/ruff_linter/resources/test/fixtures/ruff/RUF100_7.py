# flake8: noqa: F841, E501 -- used followed by unused code
# ruff: noqa: E701, F541 -- unused followed by used code


def a():
    # Violating noqa'd F841 (unused) and F541 (no-placeholder f-string)
    x = f"Hello, world"
    return 6
