from twisted.web.twcgi import CGIScript  # S412
from wsgiref.handlers import CGIHandler  # S412
from wsgiref.handlers import SomeOtherHandler  # S412
from twisted.web.twcgi import SomeOtherScript  # S412

import wsgiref.handlers  # S412
import twisted.web.twcgi  # S412

# These should not trigger S412
import wsgiref
import twisted.web
from wsgiref import simple_server
from twisted.web import resource
