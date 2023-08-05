if "sdist" in cmds:
    _sdist = cmds["sdist"]
elif "setuptools" in sys.modules:
    from setuptools.command.sdist import sdist as _sdist
else:
    from setuptools.command.sdist import sdist as _sdist
    from distutils.command.sdist import sdist as _sdist
