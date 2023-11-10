foo = None

# Error.

type(foo) is type(None)

type(None) is type(foo)

type(None) is type(None)

type(foo) is not type(None)

type(None) is not type(foo)

type(None) is not type(None)

type(foo) == type(None)

type(None) == type(foo)

type(None) == type(None)

type(foo) != type(None)

type(None) != type(foo)

type(None) != type(None)

# Ok.

foo is None

foo is not None

None is foo

None is not foo

None is None

None is not None

foo is type(None)

type(foo) is None

type(None) is None

foo is not type(None)

type(foo) is not None

type(None) is not None

foo == type(None)

type(foo) == None

type(None) == None

foo != type(None)

type(foo) != None

type(None) != None

type(foo) > type(None)
