# Test absolute imports
import xml.dom.minidom as wrong
from xml.dom import minidom as wrong
from xml.dom.minidom import parseString as wrong  # Ensure ICN001 throws on function import.

import xml.dom.minidom as md
from xml.dom import minidom as md
from xml.dom.minidom import parseString as pstr

# Test relative imports
from ..xml.dom import minidom as okay  # Ensure config "xml.dom.minidom" doesn't catch relative imports
