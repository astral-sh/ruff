# comment

class Test(
    Aaaaaaaaaaaaaaaaa,
    Bbbbbbbbbbbbbbbb,
    DDDDDDDDDDDDDDDD,
    EEEEEEEEEEEEEE,
    metaclass=meta,
):
    pass


class Test((Aaaaaaaaaaaaaaaaa), Bbbbbbbbbbbbbbbb, metaclass=meta):
    pass

class Test( # trailing class comment
    Aaaaaaaaaaaaaaaaa, # trailing comment

    # in between comment

    Bbbbbbbbbbbbbbbb,
    # another leading comment
    DDDDDDDDDDDDDDDD,
    EEEEEEEEEEEEEE,
    # meta comment
    metaclass=meta, # trailing meta comment
):
    pass

class Test((Aaaa)):
    ...


class Test(aaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbb + cccccccccccccccccccccccc + dddddddddddddddddddddd + eeeeeeeee, ffffffffffffffffff, gggggggggggggggggg):
    pass

class Test(aaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbb * cccccccccccccccccccccccc + dddddddddddddddddddddd + eeeeeeeee, ffffffffffffffffff, gggggggggggggggggg):
    pass

class TestTrailingComment1(Aaaa): # trailing comment
    pass


class TestTrailingComment2: # trailing comment
    pass


class TestTrailingComment3[T]: # trailing comment
    pass


class TestTrailingComment4[T](A): # trailing comment
    pass


class Test:
    """Docstring"""


class Test:
    # comment
    """Docstring"""


class Test:
    """Docstring"""
    x = 1


class Test:
    """Docstring"""
    # comment
    x = 1


class Test:

    """Docstring"""


class Test:
    # comment

    """Docstring"""


class Test:

    # comment

    """Docstring"""


class Test:

    """Docstring"""
    x = 1


class Test:

    """Docstring"""
    # comment
    x = 1


class C(): # comment
    pass


class C(  # comment
):
    pass


class C(
    # comment
):
    pass


class C(): # comment
    pass


class C(  # comment
    # comment
    1
):
    pass


class C(
    1
    # comment
):
    pass


@dataclass
# Copied from transformers.models.clip.modeling_clip.CLIPOutput with CLIP->AltCLIP
class AltCLIPOutput(ModelOutput):
    ...


@dataclass
class AltCLIPOutput( # Copied from transformers.models.clip.modeling_clip.CLIPOutput with CLIP->AltCLIP
):
    ...


@dataclass
class AltCLIPOutput(
    # Copied from transformers.models.clip.modeling_clip.CLIPOutput with CLIP->AltCLIP
):
    ...


class TestTypeParams[Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa, Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb, Cccccccccccccccccccccc]:
    pass


class TestTypeParams[Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa, *Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb, **Cccccccccccccccccccccc]:
    pass


class TestTypeParams[Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa]:
    pass


class TestTypeParams[*Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa]:
    pass


class TestTypeParams[**Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa]:
    pass


class TestTypeParams[**P, *Ts, T]:
    pass


class TestTypeParams[ # trailing bracket comment
    # leading comment
    A,

    # in between comment

    B,
    # another leading comment
    C,
    D, # trailing comment
    # leading bracket comment
]:
    pass



class TestTypeParams[Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa](B, C, D):
    pass



class TestTypeParams[Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa](Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb, Cccccccccccccccccccccccc, Ddddddddddddddddddddddddd):
    pass



class TestTypeParams[A, B, C](meta=Aaaaaaaaaaaaaaaaaaaaaa):
    pass


# Regression test for: https://github.com/astral-sh/ruff/pull/7001
class QuerySet(AltersData):
    """Represent a lazy database lookup for a set of objects."""

    def as_manager(cls):
        # Address the circular dependency between `Queryset` and `Manager`.
        from django.db.models.manager import Manager

        manager = Manager.from_queryset(cls)()
        manager._built_with_as_manager = True
        return manager

    as_manager.queryset_only = True
    as_manager = classmethod(as_manager)


# Decorators
@decorator
# comment
class Foo1: ...

@decorator
# comment
class Foo2(Foo1): ...

@decorator
# comment
class Foo3[T]: ...

@decorator  # comment
class Foo4: ...

@decorator
# comment
@decorato2
class Foo5: ...
