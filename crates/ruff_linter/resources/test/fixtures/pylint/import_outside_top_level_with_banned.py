def f():
    # this should be allowed due to TID253 top-level ban
    import foo_banned
    from pkg import bar_banned

    # this should still trigger an error due to multiple imports
    from pkg import foo_allowed, bar_banned
