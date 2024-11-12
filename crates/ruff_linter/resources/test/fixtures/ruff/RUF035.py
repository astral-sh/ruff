import flask
from markupsafe import Markup, escape

content = "<script>alert('Hello, world!')</script>"
Markup(f"unsafe {content}")  # RUF035
flask.Markup("unsafe {}".format(content))  # RUF035
Markup("safe {}").format(content)
flask.Markup(b"safe {}", encoding='utf-8').format(content)
escape(content)
Markup(content)  # RUF035
flask.Markup("unsafe %s" % content)  # RUF035
Markup(object="safe")
Markup(object="unsafe {}".format(content))  # Not currently detected

# NOTE: We may be able to get rid of these false positives with red-knot
#       if it includes comprehensive constant expression detection/evaluation.
Markup("*" * 8)  # RUF035 (false positive)
flask.Markup("hello {}".format("world"))  # RUF035 (false positive)
