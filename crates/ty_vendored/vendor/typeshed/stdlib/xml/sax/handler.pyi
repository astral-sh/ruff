"""
This module contains the core classes of version 2.0 of SAX for Python.
This file provides only default classes with absolutely minimum
functionality, from which drivers and applications can be subclassed.

Many of these classes are empty and are included only as documentation
of the interfaces.

$Id$
"""

import sys
from typing import Final, NoReturn, Protocol, type_check_only
from xml.sax import xmlreader

version: Final[str]

@type_check_only
class _ErrorHandlerProtocol(Protocol):  # noqa: Y046  # Protocol is not used
    def error(self, exception: BaseException) -> NoReturn: ...
    def fatalError(self, exception: BaseException) -> NoReturn: ...
    def warning(self, exception: BaseException) -> None: ...

class ErrorHandler:
    """Basic interface for SAX error handlers.

    If you create an object that implements this interface, then
    register the object with your XMLReader, the parser will call the
    methods in your object to report all warnings and errors. There
    are three levels of errors available: warnings, (possibly)
    recoverable errors, and unrecoverable errors. All methods take a
    SAXParseException as the only parameter.
    """

    def error(self, exception: BaseException) -> NoReturn:
        """Handle a recoverable error."""

    def fatalError(self, exception: BaseException) -> NoReturn:
        """Handle a non-recoverable error."""

    def warning(self, exception: BaseException) -> None:
        """Handle a warning."""

@type_check_only
class _ContentHandlerProtocol(Protocol):  # noqa: Y046  # Protocol is not used
    def setDocumentLocator(self, locator: xmlreader.Locator) -> None: ...
    def startDocument(self) -> None: ...
    def endDocument(self) -> None: ...
    def startPrefixMapping(self, prefix: str | None, uri: str) -> None: ...
    def endPrefixMapping(self, prefix: str | None) -> None: ...
    def startElement(self, name: str, attrs: xmlreader.AttributesImpl) -> None: ...
    def endElement(self, name: str) -> None: ...
    def startElementNS(self, name: tuple[str | None, str], qname: str | None, attrs: xmlreader.AttributesNSImpl) -> None: ...
    def endElementNS(self, name: tuple[str | None, str], qname: str | None) -> None: ...
    def characters(self, content: str) -> None: ...
    def ignorableWhitespace(self, whitespace: str) -> None: ...
    def processingInstruction(self, target: str, data: str) -> None: ...
    def skippedEntity(self, name: str) -> None: ...

class ContentHandler:
    """Interface for receiving logical document content events.

    This is the main callback interface in SAX, and the one most
    important to applications. The order of events in this interface
    mirrors the order of the information in the document.
    """

    def setDocumentLocator(self, locator: xmlreader.Locator) -> None:
        """Called by the parser to give the application a locator for
        locating the origin of document events.

        SAX parsers are strongly encouraged (though not absolutely
        required) to supply a locator: if it does so, it must supply
        the locator to the application by invoking this method before
        invoking any of the other methods in the DocumentHandler
        interface.

        The locator allows the application to determine the end
        position of any document-related event, even if the parser is
        not reporting an error. Typically, the application will use
        this information for reporting its own errors (such as
        character content that does not match an application's
        business rules). The information returned by the locator is
        probably not sufficient for use with a search engine.

        Note that the locator will return correct information only
        during the invocation of the events in this interface. The
        application should not attempt to use it at any other time.
        """

    def startDocument(self) -> None:
        """Receive notification of the beginning of a document.

        The SAX parser will invoke this method only once, before any
        other methods in this interface or in DTDHandler (except for
        setDocumentLocator).
        """

    def endDocument(self) -> None:
        """Receive notification of the end of a document.

        The SAX parser will invoke this method only once, and it will
        be the last method invoked during the parse. The parser shall
        not invoke this method until it has either abandoned parsing
        (because of an unrecoverable error) or reached the end of
        input.
        """

    def startPrefixMapping(self, prefix: str | None, uri: str) -> None:
        """Begin the scope of a prefix-URI Namespace mapping.

        The information from this event is not necessary for normal
        Namespace processing: the SAX XML reader will automatically
        replace prefixes for element and attribute names when the
        http://xml.org/sax/features/namespaces feature is true (the
        default).

        There are cases, however, when applications need to use
        prefixes in character data or in attribute values, where they
        cannot safely be expanded automatically; the
        start/endPrefixMapping event supplies the information to the
        application to expand prefixes in those contexts itself, if
        necessary.

        Note that start/endPrefixMapping events are not guaranteed to
        be properly nested relative to each-other: all
        startPrefixMapping events will occur before the corresponding
        startElement event, and all endPrefixMapping events will occur
        after the corresponding endElement event, but their order is
        not guaranteed.
        """

    def endPrefixMapping(self, prefix: str | None) -> None:
        """End the scope of a prefix-URI mapping.

        See startPrefixMapping for details. This event will always
        occur after the corresponding endElement event, but the order
        of endPrefixMapping events is not otherwise guaranteed.
        """

    def startElement(self, name: str, attrs: xmlreader.AttributesImpl) -> None:
        """Signals the start of an element in non-namespace mode.

        The name parameter contains the raw XML 1.0 name of the
        element type as a string and the attrs parameter holds an
        instance of the Attributes class containing the attributes of
        the element.
        """

    def endElement(self, name: str) -> None:
        """Signals the end of an element in non-namespace mode.

        The name parameter contains the name of the element type, just
        as with the startElement event.
        """

    def startElementNS(self, name: tuple[str | None, str], qname: str | None, attrs: xmlreader.AttributesNSImpl) -> None:
        """Signals the start of an element in namespace mode.

        The name parameter contains the name of the element type as a
        (uri, localname) tuple, the qname parameter the raw XML 1.0
        name used in the source document, and the attrs parameter
        holds an instance of the Attributes class containing the
        attributes of the element.

        The uri part of the name tuple is None for elements which have
        no namespace.
        """

    def endElementNS(self, name: tuple[str | None, str], qname: str | None) -> None:
        """Signals the end of an element in namespace mode.

        The name parameter contains the name of the element type, just
        as with the startElementNS event.
        """

    def characters(self, content: str) -> None:
        """Receive notification of character data.

        The Parser will call this method to report each chunk of
        character data. SAX parsers may return all contiguous
        character data in a single chunk, or they may split it into
        several chunks; however, all of the characters in any single
        event must come from the same external entity so that the
        Locator provides useful information.
        """

    def ignorableWhitespace(self, whitespace: str) -> None:
        """Receive notification of ignorable whitespace in element content.

        Validating Parsers must use this method to report each chunk
        of ignorable whitespace (see the W3C XML 1.0 recommendation,
        section 2.10): non-validating parsers may also use this method
        if they are capable of parsing and using content models.

        SAX parsers may return all contiguous whitespace in a single
        chunk, or they may split it into several chunks; however, all
        of the characters in any single event must come from the same
        external entity, so that the Locator provides useful
        information.
        """

    def processingInstruction(self, target: str, data: str) -> None:
        """Receive notification of a processing instruction.

        The Parser will invoke this method once for each processing
        instruction found: note that processing instructions may occur
        before or after the main document element.

        A SAX parser should never report an XML declaration (XML 1.0,
        section 2.8) or a text declaration (XML 1.0, section 4.3.1)
        using this method.
        """

    def skippedEntity(self, name: str) -> None:
        """Receive notification of a skipped entity.

        The Parser will invoke this method once for each entity
        skipped. Non-validating processors may skip entities if they
        have not seen the declarations (because, for example, the
        entity was declared in an external DTD subset). All processors
        may skip external entities, depending on the values of the
        http://xml.org/sax/features/external-general-entities and the
        http://xml.org/sax/features/external-parameter-entities
        properties.
        """

@type_check_only
class _DTDHandlerProtocol(Protocol):  # noqa: Y046  # Protocol is not used
    def notationDecl(self, name: str, publicId: str | None, systemId: str) -> None: ...
    def unparsedEntityDecl(self, name: str, publicId: str | None, systemId: str, ndata: str) -> None: ...

class DTDHandler:
    """Handle DTD events.

    This interface specifies only those DTD events required for basic
    parsing (unparsed entities and attributes).
    """

    def notationDecl(self, name: str, publicId: str | None, systemId: str) -> None:
        """Handle a notation declaration event."""

    def unparsedEntityDecl(self, name: str, publicId: str | None, systemId: str, ndata: str) -> None:
        """Handle an unparsed entity declaration event."""

@type_check_only
class _EntityResolverProtocol(Protocol):  # noqa: Y046  # Protocol is not used
    def resolveEntity(self, publicId: str | None, systemId: str) -> str: ...

class EntityResolver:
    """Basic interface for resolving entities. If you create an object
    implementing this interface, then register the object with your
    Parser, the parser will call the method in your object to
    resolve all external entities. Note that DefaultHandler implements
    this interface with the default behaviour.
    """

    def resolveEntity(self, publicId: str | None, systemId: str) -> str:
        """Resolve the system identifier of an entity and return either
        the system identifier to read from as a string, or an InputSource
        to read from.
        """

feature_namespaces: Final = "http://xml.org/sax/features/namespaces"
feature_namespace_prefixes: Final = "http://xml.org/sax/features/namespace-prefixes"
feature_string_interning: Final = "http://xml.org/sax/features/string-interning"
feature_validation: Final = "http://xml.org/sax/features/validation"
feature_external_ges: Final[str]  # too long string
feature_external_pes: Final[str]  # too long string
all_features: Final[list[str]]
property_lexical_handler: Final = "http://xml.org/sax/properties/lexical-handler"
property_declaration_handler: Final = "http://xml.org/sax/properties/declaration-handler"
property_dom_node: Final = "http://xml.org/sax/properties/dom-node"
property_xml_string: Final = "http://xml.org/sax/properties/xml-string"
property_encoding: Final = "http://www.python.org/sax/properties/encoding"
property_interning_dict: Final[str]  # too long string
all_properties: Final[list[str]]

if sys.version_info >= (3, 10):
    class LexicalHandler:
        """Optional SAX2 handler for lexical events.

        This handler is used to obtain lexical information about an XML
        document, that is, information about how the document was encoded
        (as opposed to what it contains, which is reported to the
        ContentHandler), such as comments and CDATA marked section
        boundaries.

        To set the LexicalHandler of an XMLReader, use the setProperty
        method with the property identifier
        'http://xml.org/sax/properties/lexical-handler'.
        """

        def comment(self, content: str) -> None:
            """Reports a comment anywhere in the document (including the
            DTD and outside the document element).

            content is a string that holds the contents of the comment.
            """

        def startDTD(self, name: str, public_id: str | None, system_id: str | None) -> None:
            """Report the start of the DTD declarations, if the document
            has an associated DTD.

            A startEntity event will be reported before declaration events
            from the external DTD subset are reported, and this can be
            used to infer from which subset DTD declarations derive.

            name is the name of the document element type, public_id the
            public identifier of the DTD (or None if none were supplied)
            and system_id the system identifier of the external subset (or
            None if none were supplied).
            """

        def endDTD(self) -> None:
            """Signals the end of DTD declarations."""

        def startCDATA(self) -> None:
            """Reports the beginning of a CDATA marked section.

            The contents of the CDATA marked section will be reported
            through the characters event.
            """

        def endCDATA(self) -> None:
            """Reports the end of a CDATA marked section."""
