def __badAllowed__():
    pass

def __stillBad__():
    pass


def nested():
    def __badAllowed__():
        pass

    def __stillBad__():
        pass
