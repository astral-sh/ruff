import flask
from markupsafe import Markup, escape

content = "<script>alert('Hello, world!')</script>"
Markup(f"unsafe {content}")  # MS001
flask.Markup("unsafe {}".format(content))  # MS001
Markup("safe {}").format(content)
flask.Markup(b"safe {}", encoding='utf-8').format(content)
escape(content)
Markup(content)  # MS001
flask.Markup("unsafe %s" % content)  # MS001
Markup(object="safe")
Markup(object="unsafe {}".format(content))  # Not currently detected

# NOTE: We may be able to get rid of these false positives with red-knot
#       if it includes comprehensive constant expression detection/evaluation.
Markup("*" * 8)  # MS001 (false positive)
flask.Markup("hello {}".format("world"))  # MS001 (false positive)
