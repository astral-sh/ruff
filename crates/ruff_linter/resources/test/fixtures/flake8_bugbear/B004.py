def this_is_a_bug():
    o = object()
    if hasattr(o, "__call__"):
        print("Ooh, callable! Or is it?")
    if getattr(o, "__call__", False):
        print("Ooh, callable! Or is it?")


def still_a_bug():
    import builtins
    o = object()
    if builtins.hasattr(o, "__call__"):
        print("B U G")
    if builtins.getattr(o, "__call__", False):
        print("B   U   G")


def trickier_fix_for_this_one():
    o = object()

    def callable(x):
        return True

    if hasattr(o, "__call__"):
        print("STILL a bug!")


def this_is_fine():
    o = object()
    if callable(o):
        print("Ooh, this is actually callable.")
