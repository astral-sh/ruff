class ClassSimplest:
    pass
class ClassWithSingleField:
    a = 1
class ClassWithJustTheDocstring:
    """Just a docstring."""
class ClassWithInit:
    def __init__(self):
        pass
class ClassWithTheDocstringAndInit:
    """Just a docstring."""
    def __init__(self):
        pass
class ClassWithInitAndVars:
    cls_var = 100
    def __init__(self):
        pass
class ClassWithInitAndVarsAndDocstring:
    """Test class"""
    cls_var = 100
    def __init__(self):
        pass
class ClassWithDecoInit:
    @deco
    def __init__(self):
        pass
class ClassWithDecoInitAndVars:
    cls_var = 100
    @deco
    def __init__(self):
        pass
class ClassWithDecoInitAndVarsAndDocstring:
    """Test class"""
    cls_var = 100
    @deco
    def __init__(self):
        pass
class ClassSimplestWithInner:
    class Inner:
        pass
class ClassSimplestWithInnerWithDocstring:
    class Inner:
        """Just a docstring."""
        def __init__(self):
            pass
class ClassWithSingleFieldWithInner:
    a = 1
    class Inner:
        pass
class ClassWithJustTheDocstringWithInner:
    """Just a docstring."""
    class Inner:
        pass
class ClassWithInitWithInner:
    class Inner:
        pass
    def __init__(self):
        pass
class ClassWithInitAndVarsWithInner:
    cls_var = 100
    class Inner:
        pass
    def __init__(self):
        pass
class ClassWithInitAndVarsAndDocstringWithInner:
    """Test class"""
    cls_var = 100
    class Inner:
        pass
    def __init__(self):
        pass
class ClassWithDecoInitWithInner:
    class Inner:
        pass
    @deco
    def __init__(self):
        pass
class ClassWithDecoInitAndVarsWithInner:
    cls_var = 100
    class Inner:
        pass
    @deco
    def __init__(self):
        pass
class ClassWithDecoInitAndVarsAndDocstringWithInner:
    """Test class"""
    cls_var = 100
    class Inner:
        pass
    @deco
    def __init__(self):
        pass
class ClassWithDecoInitAndVarsAndDocstringWithInner2:
    """Test class"""
    class Inner:
        pass
    cls_var = 100
    @deco
    def __init__(self):
        pass
