class Top1:
    pass
class Top2:
    pass

class Top:
    class Ellipsis: ...
    class Ellipsis: ...

class Top:
    class Ellipsis: ...
    class Pass:
        pass

class Top:
    class Ellipsis: ...
    class_variable = 1

class Top:
    class TrailingComment:
        pass
    # comment
    class Other:
        pass

class Top:
    class CommentWithEllipsis: ...
    # comment
    class Other: ...

class Top:
    class TrailingCommentWithMultipleBlankLines:
        pass


    # comment
    class Other:
        pass

class Top:
    class Nested:
        pass

    # comment
    class LeadingComment:
        pass

class Top:
    @decorator
    class Ellipsis: ...
    class Ellipsis: ...

class Top:
    @decorator
    class Ellipsis: ...
    @decorator
    class Ellipsis: ...

class Top:
    @decorator
    class Ellipsis: ...
    @decorator
    class Pass:
        pass

class Top:
    class Foo:
        pass




    class AfterMultipleEmptyLines:
        pass

class Top:
    class Nested11:
        class Nested12:
            pass
    class Nested21:
        pass

class Top:
    class Nested11:
        class Nested12:
            pass
        # comment
    class Nested21:
        pass

class Top:
    class Nested11:
        class Nested12:
            pass
    # comment
    class Nested21:
        pass
    # comment

class Top1:
    class Nested:
        pass
class Top2:
    pass

class Top1:
    class Nested:
        pass
    # comment
class Top2:
    pass

class Top1:
    class Nested:
        pass
# comment
class Top2:
    pass

if foo:
    class Nested1:
        pass
    class Nested2:
        pass
else:
    pass

if foo:
    class Nested1:
        pass
    class Nested2:
        pass
    # comment
elif bar:
    class Nested1:
        pass
# comment
else:
    pass

if top1:
    class Nested:
        pass
if top2:
    pass

if top1:
    class Nested:
        pass
    # comment
if top2:
    pass

if top1:
    class Nested:
        pass
# comment
if top2:
    pass

try:
    class Try:
        pass
except:
    class Except:
        pass
foo = 1

match foo:
    case 1:
        class Nested:
            pass
    case 2:
        class Nested:
            pass
    case _:
        class Nested:
            pass
foo = 1

class Eof:
    class Nested:
        pass
