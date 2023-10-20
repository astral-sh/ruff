# These two imports have something after cElementTree, so they should be fixed.
from xml.etree.cElementTree import XML, Element, SubElement
import xml.etree.cElementTree as ET

# Weird spacing should not cause issues.
from   xml.etree.cElementTree    import  XML
import    xml.etree.cElementTree       as      ET

# Multi line imports should also work fine.
from xml.etree.cElementTree import (
    XML,
    Element,
    SubElement,
)
if True:
    import xml.etree.cElementTree as ET
    from xml.etree import cElementTree as CET

from xml.etree import cElementTree as ET

import contextlib, xml.etree.cElementTree as ET

# This should fix the second, but not the first invocation.
import xml.etree.cElementTree, xml.etree.cElementTree as ET

# The below items should NOT be changed.
import xml.etree.cElementTree

from .xml.etree.cElementTree import XML

from xml.etree import cElementTree
