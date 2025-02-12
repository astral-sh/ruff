### Errors

_ = [
    "a"
    "b"
]


_ = {a, "b" "c"      # This part is caught by ISC001
        "d", e}      # This part isn't


_ = [
    "a", b, "c"
    "d", e, "f"
]


_ = (
    f"{a}"
    f"{b}"
)


_ = {
    "a",
    f"{b}"
    "c",
    d
}


_ = [
    "a"
    "b"
    f"{c}",
    "d"
]


_ = (
    '''
    lorem
    ipsum
    '''

    f"""
    {foo}
    bar
    """,
    d
)


# https://github.com/astral-sh/ruff/issues/13014
cmd = [
    "rsync",
    "-avz",  # archive mode, verbose, compress
    "-e",
    "ssh",
    "--exclude=.git"  # equivalent to ignore-vcs
    "--delete",
    "/root/code/api/",
    "user@remote:/app/"
    # Preserve symlinks as-is  (equivalent to --symlink-mode=posix-raw)
    "--links",
]


### No errors

_ = [
    "lorem",
    (
        "foo"
        "bar"
    ),
    dolor
]

_ = (
    """a""",
    """
    b
    c
    """,
    '''
    d
    '''
)

# Already caught by ISC001
_ = (a, "b" "c", d)
_ = (a, """
    b
    """'''
    c
    ''', d)
