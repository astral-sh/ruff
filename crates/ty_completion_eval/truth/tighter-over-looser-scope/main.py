scope1 = 1
def x():
    scope2 = 1
    def xx():
        scope3 = 1
        def xxx():
            # We specifically want `scope3` to be
            # suggested first here, since that's in
            # the "tighter" scope.
            scope<CURSOR: scope3>
