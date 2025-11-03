"""Header value parser implementing various email-related RFC parsing rules.

The parsing methods defined in this module implement various email related
parsing rules.  Principal among them is RFC 5322, which is the followon
to RFC 2822 and primarily a clarification of the former.  It also implements
RFC 2047 encoded word decoding.

RFC 5322 goes to considerable trouble to maintain backward compatibility with
RFC 822 in the parse phase, while cleaning up the structure on the generation
phase.  This parser supports correct RFC 5322 generation by tagging white space
as folding white space only when folding is allowed in the non-obsolete rule
sets.  Actually, the parser is even more generous when accepting input than RFC
5322 mandates, following the spirit of Postel's Law, which RFC 5322 encourages.
Where possible deviations from the standard are annotated on the 'defects'
attribute of tokens that deviate.

The general structure of the parser follows RFC 5322, and uses its terminology
where there is a direct correspondence.  Where the implementation requires a
somewhat different structure than that used by the formal grammar, new terms
that mimic the closest existing terms are used.  Thus, it really helps to have
a copy of RFC 5322 handy when studying this code.

Input to the parser is a string that has already been unfolded according to
RFC 5322 rules.  According to the RFC this unfolding is the very first step, and
this parser leaves the unfolding step to a higher level message parser, which
will have already detected the line breaks that need unfolding while
determining the beginning and end of each header.

The output of the parser is a TokenList object, which is a list subclass.  A
TokenList is a recursive data structure.  The terminal nodes of the structure
are Terminal objects, which are subclasses of str.  These do not correspond
directly to terminal objects in the formal grammar, but are instead more
practical higher level combinations of true terminals.

All TokenList and Terminal objects have a 'value' attribute, which produces the
semantically meaningful value of that part of the parse subtree.  The value of
all whitespace tokens (no matter how many sub-tokens they may contain) is a
single space, as per the RFC rules.  This includes 'CFWS', which is herein
included in the general class of whitespace tokens.  There is one exception to
the rule that whitespace tokens are collapsed into single spaces in values: in
the value of a 'bare-quoted-string' (a quoted-string with no leading or
trailing whitespace), any whitespace that appeared between the quotation marks
is preserved in the returned value.  Note that in all Terminal strings quoted
pairs are turned into their unquoted values.

All TokenList and Terminal objects also have a string value, which attempts to
be a "canonical" representation of the RFC-compliant form of the substring that
produced the parsed subtree, including minimal use of quoted pair quoting.
Whitespace runs are not collapsed.

Comment tokens also have a 'content' attribute providing the string found
between the parens (including any nested comments) with whitespace preserved.

All TokenList and Terminal objects have a 'defects' attribute which is a
possibly empty list all of the defects found while creating the token.  Defects
may appear on any token in the tree, and a composite list of all defects in the
subtree is available through the 'all_defects' attribute of any node.  (For
Terminal notes x.defects == x.all_defects.)

Each object in a parse tree is called a 'token', and each has a 'token_type'
attribute that gives the name from the RFC 5322 grammar that it represents.
Not all RFC 5322 nodes are produced, and there is one non-RFC 5322 node that
may be produced: 'ptext'.  A 'ptext' is a string of printable ascii characters.
It is returned in place of lists of (ctext/quoted-pair) and
(qtext/quoted-pair).

XXX: provide complete list of token types.
"""

from collections.abc import Iterable, Iterator
from email.errors import HeaderParseError, MessageDefect
from email.policy import Policy
from re import Pattern
from typing import Any, Final
from typing_extensions import Self

WSP: Final[set[str]]
CFWS_LEADER: Final[set[str]]
SPECIALS: Final[set[str]]
ATOM_ENDS: Final[set[str]]
DOT_ATOM_ENDS: Final[set[str]]
PHRASE_ENDS: Final[set[str]]
TSPECIALS: Final[set[str]]
TOKEN_ENDS: Final[set[str]]
ASPECIALS: Final[set[str]]
ATTRIBUTE_ENDS: Final[set[str]]
EXTENDED_ATTRIBUTE_ENDS: Final[set[str]]
# Added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
NLSET: Final[set[str]]
# Added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
SPECIALSNL: Final[set[str]]

# Added in Python 3.9.23, 3.10.17, 3.11.12, 3.12.9, 3.13.2
def make_quoted_pairs(value: Any) -> str:
    """Escape dquote and backslash for use within a quoted-string."""

def quote_string(value: Any) -> str: ...

rfc2047_matcher: Final[Pattern[str]]

class TokenList(list[TokenList | Terminal]):
    token_type: str | None
    syntactic_break: bool
    ew_combine_allowed: bool
    defects: list[MessageDefect]
    def __init__(self, *args: Any, **kw: Any) -> None: ...
    @property
    def value(self) -> str: ...
    @property
    def all_defects(self) -> list[MessageDefect]: ...
    def startswith_fws(self) -> bool: ...
    @property
    def as_ew_allowed(self) -> bool:
        """True if all top level tokens of this part may be RFC2047 encoded."""

    @property
    def comments(self) -> list[str]: ...
    def fold(self, *, policy: Policy) -> str: ...
    def pprint(self, indent: str = "") -> None: ...
    def ppstr(self, indent: str = "") -> str: ...

class WhiteSpaceTokenList(TokenList): ...

class UnstructuredTokenList(TokenList):
    token_type: str

class Phrase(TokenList):
    token_type: str

class Word(TokenList):
    token_type: str

class CFWSList(WhiteSpaceTokenList):
    token_type: str

class Atom(TokenList):
    token_type: str

class Token(TokenList):
    token_type: str
    encode_as_ew: bool

class EncodedWord(TokenList):
    token_type: str
    cte: str | None
    charset: str | None
    lang: str | None

class QuotedString(TokenList):
    token_type: str
    @property
    def content(self) -> str: ...
    @property
    def quoted_value(self) -> str: ...
    @property
    def stripped_value(self) -> str: ...

class BareQuotedString(QuotedString):
    token_type: str

class Comment(WhiteSpaceTokenList):
    token_type: str
    def quote(self, value: Any) -> str: ...
    @property
    def content(self) -> str: ...

class AddressList(TokenList):
    token_type: str
    @property
    def addresses(self) -> list[Address]: ...
    @property
    def mailboxes(self) -> list[Mailbox]: ...
    @property
    def all_mailboxes(self) -> list[Mailbox]: ...

class Address(TokenList):
    token_type: str
    @property
    def display_name(self) -> str: ...
    @property
    def mailboxes(self) -> list[Mailbox]: ...
    @property
    def all_mailboxes(self) -> list[Mailbox]: ...

class MailboxList(TokenList):
    token_type: str
    @property
    def mailboxes(self) -> list[Mailbox]: ...
    @property
    def all_mailboxes(self) -> list[Mailbox]: ...

class GroupList(TokenList):
    token_type: str
    @property
    def mailboxes(self) -> list[Mailbox]: ...
    @property
    def all_mailboxes(self) -> list[Mailbox]: ...

class Group(TokenList):
    token_type: str
    @property
    def mailboxes(self) -> list[Mailbox]: ...
    @property
    def all_mailboxes(self) -> list[Mailbox]: ...
    @property
    def display_name(self) -> str: ...

class NameAddr(TokenList):
    token_type: str
    @property
    def display_name(self) -> str: ...
    @property
    def local_part(self) -> str: ...
    @property
    def domain(self) -> str: ...
    @property
    def route(self) -> list[Domain] | None: ...
    @property
    def addr_spec(self) -> str: ...

class AngleAddr(TokenList):
    token_type: str
    @property
    def local_part(self) -> str: ...
    @property
    def domain(self) -> str: ...
    @property
    def route(self) -> list[Domain] | None: ...
    @property
    def addr_spec(self) -> str: ...

class ObsRoute(TokenList):
    token_type: str
    @property
    def domains(self) -> list[Domain]: ...

class Mailbox(TokenList):
    token_type: str
    @property
    def display_name(self) -> str: ...
    @property
    def local_part(self) -> str: ...
    @property
    def domain(self) -> str: ...
    @property
    def route(self) -> list[str]: ...
    @property
    def addr_spec(self) -> str: ...

class InvalidMailbox(TokenList):
    token_type: str
    @property
    def display_name(self) -> None: ...
    @property
    def local_part(self) -> None: ...
    @property
    def domain(self) -> None: ...
    @property
    def route(self) -> None: ...
    @property
    def addr_spec(self) -> None: ...

class Domain(TokenList):
    token_type: str
    as_ew_allowed: bool
    @property
    def domain(self) -> str: ...

class DotAtom(TokenList):
    token_type: str

class DotAtomText(TokenList):
    token_type: str
    as_ew_allowed: bool

class NoFoldLiteral(TokenList):
    token_type: str
    as_ew_allowed: bool

class AddrSpec(TokenList):
    token_type: str
    as_ew_allowed: bool
    @property
    def local_part(self) -> str: ...
    @property
    def domain(self) -> str: ...
    @property
    def addr_spec(self) -> str: ...

class ObsLocalPart(TokenList):
    token_type: str
    as_ew_allowed: bool

class DisplayName(Phrase):
    token_type: str
    @property
    def display_name(self) -> str: ...

class LocalPart(TokenList):
    token_type: str
    as_ew_allowed: bool
    @property
    def local_part(self) -> str: ...

class DomainLiteral(TokenList):
    token_type: str
    as_ew_allowed: bool
    @property
    def domain(self) -> str: ...
    @property
    def ip(self) -> str: ...

class MIMEVersion(TokenList):
    token_type: str
    major: int | None
    minor: int | None

class Parameter(TokenList):
    token_type: str
    sectioned: bool
    extended: bool
    charset: str
    @property
    def section_number(self) -> int: ...
    @property
    def param_value(self) -> str: ...

class InvalidParameter(Parameter):
    token_type: str

class Attribute(TokenList):
    token_type: str
    @property
    def stripped_value(self) -> str: ...

class Section(TokenList):
    token_type: str
    number: int | None

class Value(TokenList):
    token_type: str
    @property
    def stripped_value(self) -> str: ...

class MimeParameters(TokenList):
    token_type: str
    syntactic_break: bool
    @property
    def params(self) -> Iterator[tuple[str, str]]: ...

class ParameterizedHeaderValue(TokenList):
    syntactic_break: bool
    @property
    def params(self) -> Iterable[tuple[str, str]]: ...

class ContentType(ParameterizedHeaderValue):
    token_type: str
    as_ew_allowed: bool
    maintype: str
    subtype: str

class ContentDisposition(ParameterizedHeaderValue):
    token_type: str
    as_ew_allowed: bool
    content_disposition: Any

class ContentTransferEncoding(TokenList):
    token_type: str
    as_ew_allowed: bool
    cte: str

class HeaderLabel(TokenList):
    token_type: str
    as_ew_allowed: bool

class MsgID(TokenList):
    token_type: str
    as_ew_allowed: bool
    def fold(self, policy: Policy) -> str: ...

class MessageID(MsgID):
    token_type: str

class InvalidMessageID(MessageID):
    token_type: str

class Header(TokenList):
    token_type: str

class Terminal(str):
    as_ew_allowed: bool
    ew_combine_allowed: bool
    syntactic_break: bool
    token_type: str
    defects: list[MessageDefect]
    def __new__(cls, value: str, token_type: str) -> Self: ...
    def pprint(self) -> None: ...
    @property
    def all_defects(self) -> list[MessageDefect]: ...
    def pop_trailing_ws(self) -> None: ...
    @property
    def comments(self) -> list[str]: ...
    def __getnewargs__(self) -> tuple[str, str]: ...  # type: ignore[override]

class WhiteSpaceTerminal(Terminal):
    @property
    def value(self) -> str: ...
    def startswith_fws(self) -> bool: ...

class ValueTerminal(Terminal):
    @property
    def value(self) -> ValueTerminal: ...
    def startswith_fws(self) -> bool: ...

class EWWhiteSpaceTerminal(WhiteSpaceTerminal): ...

class _InvalidEwError(HeaderParseError):
    """Invalid encoded word found while parsing headers."""

DOT: Final[ValueTerminal]
ListSeparator: Final[ValueTerminal]
RouteComponentMarker: Final[ValueTerminal]

def get_fws(value: str) -> tuple[WhiteSpaceTerminal, str]:
    """FWS = 1*WSP

    This isn't the RFC definition.  We're using fws to represent tokens where
    folding can be done, but when we are parsing the *un*folding has already
    been done so we don't need to watch out for CRLF.

    """

def get_encoded_word(value: str, terminal_type: str = "vtext") -> tuple[EncodedWord, str]:
    """encoded-word = "=?" charset "?" encoding "?" encoded-text "?=" """

def get_unstructured(value: str) -> UnstructuredTokenList:
    """unstructured = (*([FWS] vchar) *WSP) / obs-unstruct
       obs-unstruct = *((*LF *CR *(obs-utext) *LF *CR)) / FWS)
       obs-utext = %d0 / obs-NO-WS-CTL / LF / CR

       obs-NO-WS-CTL is control characters except WSP/CR/LF.

    So, basically, we have printable runs, plus control characters or nulls in
    the obsolete syntax, separated by whitespace.  Since RFC 2047 uses the
    obsolete syntax in its specification, but requires whitespace on either
    side of the encoded words, I can see no reason to need to separate the
    non-printable-non-whitespace from the printable runs if they occur, so we
    parse this into xtext tokens separated by WSP tokens.

    Because an 'unstructured' value must by definition constitute the entire
    value, this 'get' routine does not return a remaining value, only the
    parsed TokenList.

    """

def get_qp_ctext(value: str) -> tuple[WhiteSpaceTerminal, str]:
    """ctext = <printable ascii except \\ ( )>

    This is not the RFC ctext, since we are handling nested comments in comment
    and unquoting quoted-pairs here.  We allow anything except the '()'
    characters, but if we find any ASCII other than the RFC defined printable
    ASCII, a NonPrintableDefect is added to the token's defects list.  Since
    quoted pairs are converted to their unquoted values, what is returned is
    a 'ptext' token.  In this case it is a WhiteSpaceTerminal, so it's value
    is ' '.

    """

def get_qcontent(value: str) -> tuple[ValueTerminal, str]:
    """qcontent = qtext / quoted-pair

    We allow anything except the DQUOTE character, but if we find any ASCII
    other than the RFC defined printable ASCII, a NonPrintableDefect is
    added to the token's defects list.  Any quoted pairs are converted to their
    unquoted values, so what is returned is a 'ptext' token.  In this case it
    is a ValueTerminal.

    """

def get_atext(value: str) -> tuple[ValueTerminal, str]:
    """atext = <matches _atext_matcher>

    We allow any non-ATOM_ENDS in atext, but add an InvalidATextDefect to
    the token's defects list if we find non-atext characters.
    """

def get_bare_quoted_string(value: str) -> tuple[BareQuotedString, str]:
    """bare-quoted-string = DQUOTE *([FWS] qcontent) [FWS] DQUOTE

    A quoted-string without the leading or trailing white space.  Its
    value is the text between the quote marks, with whitespace
    preserved and quoted pairs decoded.
    """

def get_comment(value: str) -> tuple[Comment, str]:
    """comment = "(" *([FWS] ccontent) [FWS] ")"
       ccontent = ctext / quoted-pair / comment

    We handle nested comments here, and quoted-pair in our qp-ctext routine.
    """

def get_cfws(value: str) -> tuple[CFWSList, str]:
    """CFWS = (1*([FWS] comment) [FWS]) / FWS"""

def get_quoted_string(value: str) -> tuple[QuotedString, str]:
    """quoted-string = [CFWS] <bare-quoted-string> [CFWS]

    'bare-quoted-string' is an intermediate class defined by this
    parser and not by the RFC grammar.  It is the quoted string
    without any attached CFWS.
    """

def get_atom(value: str) -> tuple[Atom, str]:
    """atom = [CFWS] 1*atext [CFWS]

    An atom could be an rfc2047 encoded word.
    """

def get_dot_atom_text(value: str) -> tuple[DotAtomText, str]:
    """dot-text = 1*atext *("." 1*atext)"""

def get_dot_atom(value: str) -> tuple[DotAtom, str]:
    """dot-atom = [CFWS] dot-atom-text [CFWS]

    Any place we can have a dot atom, we could instead have an rfc2047 encoded
    word.
    """

def get_word(value: str) -> tuple[Any, str]:
    """word = atom / quoted-string

    Either atom or quoted-string may start with CFWS.  We have to peel off this
    CFWS first to determine which type of word to parse.  Afterward we splice
    the leading CFWS, if any, into the parsed sub-token.

    If neither an atom or a quoted-string is found before the next special, a
    HeaderParseError is raised.

    The token returned is either an Atom or a QuotedString, as appropriate.
    This means the 'word' level of the formal grammar is not represented in the
    parse tree; this is because having that extra layer when manipulating the
    parse tree is more confusing than it is helpful.

    """

def get_phrase(value: str) -> tuple[Phrase, str]:
    """phrase = 1*word / obs-phrase
        obs-phrase = word *(word / "." / CFWS)

    This means a phrase can be a sequence of words, periods, and CFWS in any
    order as long as it starts with at least one word.  If anything other than
    words is detected, an ObsoleteHeaderDefect is added to the token's defect
    list.  We also accept a phrase that starts with CFWS followed by a dot;
    this is registered as an InvalidHeaderDefect, since it is not supported by
    even the obsolete grammar.

    """

def get_local_part(value: str) -> tuple[LocalPart, str]:
    """local-part = dot-atom / quoted-string / obs-local-part"""

def get_obs_local_part(value: str) -> tuple[ObsLocalPart, str]:
    """obs-local-part = word *("." word)"""

def get_dtext(value: str) -> tuple[ValueTerminal, str]:
    """dtext = <printable ascii except \\ [ ]> / obs-dtext
        obs-dtext = obs-NO-WS-CTL / quoted-pair

    We allow anything except the excluded characters, but if we find any
    ASCII other than the RFC defined printable ASCII, a NonPrintableDefect is
    added to the token's defects list.  Quoted pairs are converted to their
    unquoted values, so what is returned is a ptext token, in this case a
    ValueTerminal.  If there were quoted-printables, an ObsoleteHeaderDefect is
    added to the returned token's defect list.

    """

def get_domain_literal(value: str) -> tuple[DomainLiteral, str]:
    """domain-literal = [CFWS] "[" *([FWS] dtext) [FWS] "]" [CFWS]"""

def get_domain(value: str) -> tuple[Domain, str]:
    """domain = dot-atom / domain-literal / obs-domain
    obs-domain = atom *("." atom))

    """

def get_addr_spec(value: str) -> tuple[AddrSpec, str]:
    """addr-spec = local-part "@" domain"""

def get_obs_route(value: str) -> tuple[ObsRoute, str]:
    """obs-route = obs-domain-list ":"
    obs-domain-list = *(CFWS / ",") "@" domain *("," [CFWS] ["@" domain])

    Returns an obs-route token with the appropriate sub-tokens (that is,
    there is no obs-domain-list in the parse tree).
    """

def get_angle_addr(value: str) -> tuple[AngleAddr, str]:
    """angle-addr = [CFWS] "<" addr-spec ">" [CFWS] / obs-angle-addr
    obs-angle-addr = [CFWS] "<" obs-route addr-spec ">" [CFWS]

    """

def get_display_name(value: str) -> tuple[DisplayName, str]:
    """display-name = phrase

    Because this is simply a name-rule, we don't return a display-name
    token containing a phrase, but rather a display-name token with
    the content of the phrase.

    """

def get_name_addr(value: str) -> tuple[NameAddr, str]:
    """name-addr = [display-name] angle-addr"""

def get_mailbox(value: str) -> tuple[Mailbox, str]:
    """mailbox = name-addr / addr-spec"""

def get_invalid_mailbox(value: str, endchars: str) -> tuple[InvalidMailbox, str]:
    """Read everything up to one of the chars in endchars.

    This is outside the formal grammar.  The InvalidMailbox TokenList that is
    returned acts like a Mailbox, but the data attributes are None.

    """

def get_mailbox_list(value: str) -> tuple[MailboxList, str]:
    """mailbox-list = (mailbox *("," mailbox)) / obs-mbox-list
        obs-mbox-list = *([CFWS] ",") mailbox *("," [mailbox / CFWS])

    For this routine we go outside the formal grammar in order to improve error
    handling.  We recognize the end of the mailbox list only at the end of the
    value or at a ';' (the group terminator).  This is so that we can turn
    invalid mailboxes into InvalidMailbox tokens and continue parsing any
    remaining valid mailboxes.  We also allow all mailbox entries to be null,
    and this condition is handled appropriately at a higher level.

    """

def get_group_list(value: str) -> tuple[GroupList, str]:
    """group-list = mailbox-list / CFWS / obs-group-list
    obs-group-list = 1*([CFWS] ",") [CFWS]

    """

def get_group(value: str) -> tuple[Group, str]:
    """group = display-name ":" [group-list] ";" [CFWS]"""

def get_address(value: str) -> tuple[Address, str]:
    """address = mailbox / group

    Note that counter-intuitively, an address can be either a single address or
    a list of addresses (a group).  This is why the returned Address object has
    a 'mailboxes' attribute which treats a single address as a list of length
    one.  When you need to differentiate between to two cases, extract the single
    element, which is either a mailbox or a group token.

    """

def get_address_list(value: str) -> tuple[AddressList, str]:
    """address_list = (address *("," address)) / obs-addr-list
        obs-addr-list = *([CFWS] ",") address *("," [address / CFWS])

    We depart from the formal grammar here by continuing to parse until the end
    of the input, assuming the input to be entirely composed of an
    address-list.  This is always true in email parsing, and allows us
    to skip invalid addresses to parse additional valid ones.

    """

def get_no_fold_literal(value: str) -> tuple[NoFoldLiteral, str]:
    """no-fold-literal = "[" *dtext "]" """

def get_msg_id(value: str) -> tuple[MsgID, str]:
    """msg-id = [CFWS] "<" id-left '@' id-right  ">" [CFWS]
    id-left = dot-atom-text / obs-id-left
    id-right = dot-atom-text / no-fold-literal / obs-id-right
    no-fold-literal = "[" *dtext "]"
    """

def parse_message_id(value: str) -> MessageID:
    """message-id      =   "Message-ID:" msg-id CRLF"""

def parse_mime_version(value: str) -> MIMEVersion:
    """mime-version = [CFWS] 1*digit [CFWS] "." [CFWS] 1*digit [CFWS]"""

def get_invalid_parameter(value: str) -> tuple[InvalidParameter, str]:
    """Read everything up to the next ';'.

    This is outside the formal grammar.  The InvalidParameter TokenList that is
    returned acts like a Parameter, but the data attributes are None.

    """

def get_ttext(value: str) -> tuple[ValueTerminal, str]:
    """ttext = <matches _ttext_matcher>

    We allow any non-TOKEN_ENDS in ttext, but add defects to the token's
    defects list if we find non-ttext characters.  We also register defects for
    *any* non-printables even though the RFC doesn't exclude all of them,
    because we follow the spirit of RFC 5322.

    """

def get_token(value: str) -> tuple[Token, str]:
    """token = [CFWS] 1*ttext [CFWS]

    The RFC equivalent of ttext is any US-ASCII chars except space, ctls, or
    tspecials.  We also exclude tabs even though the RFC doesn't.

    The RFC implies the CFWS but is not explicit about it in the BNF.

    """

def get_attrtext(value: str) -> tuple[ValueTerminal, str]:
    """attrtext = 1*(any non-ATTRIBUTE_ENDS character)

    We allow any non-ATTRIBUTE_ENDS in attrtext, but add defects to the
    token's defects list if we find non-attrtext characters.  We also register
    defects for *any* non-printables even though the RFC doesn't exclude all of
    them, because we follow the spirit of RFC 5322.

    """

def get_attribute(value: str) -> tuple[Attribute, str]:
    """[CFWS] 1*attrtext [CFWS]

    This version of the BNF makes the CFWS explicit, and as usual we use a
    value terminal for the actual run of characters.  The RFC equivalent of
    attrtext is the token characters, with the subtraction of '*', "'", and '%'.
    We include tab in the excluded set just as we do for token.

    """

def get_extended_attrtext(value: str) -> tuple[ValueTerminal, str]:
    """attrtext = 1*(any non-ATTRIBUTE_ENDS character plus '%')

    This is a special parsing routine so that we get a value that
    includes % escapes as a single string (which we decode as a single
    string later).

    """

def get_extended_attribute(value: str) -> tuple[Attribute, str]:
    """[CFWS] 1*extended_attrtext [CFWS]

    This is like the non-extended version except we allow % characters, so that
    we can pick up an encoded value as a single string.

    """

def get_section(value: str) -> tuple[Section, str]:
    """'*' digits

    The formal BNF is more complicated because leading 0s are not allowed.  We
    check for that and add a defect.  We also assume no CFWS is allowed between
    the '*' and the digits, though the RFC is not crystal clear on that.
    The caller should already have dealt with leading CFWS.

    """

def get_value(value: str) -> tuple[Value, str]:
    """quoted-string / attribute"""

def get_parameter(value: str) -> tuple[Parameter, str]:
    """attribute [section] ["*"] [CFWS] "=" value

    The CFWS is implied by the RFC but not made explicit in the BNF.  This
    simplified form of the BNF from the RFC is made to conform with the RFC BNF
    through some extra checks.  We do it this way because it makes both error
    recovery and working with the resulting parse tree easier.
    """

def parse_mime_parameters(value: str) -> MimeParameters:
    """parameter *( ";" parameter )

    That BNF is meant to indicate this routine should only be called after
    finding and handling the leading ';'.  There is no corresponding rule in
    the formal RFC grammar, but it is more convenient for us for the set of
    parameters to be treated as its own TokenList.

    This is 'parse' routine because it consumes the remaining value, but it
    would never be called to parse a full header.  Instead it is called to
    parse everything after the non-parameter value of a specific MIME header.

    """

def parse_content_type_header(value: str) -> ContentType:
    """maintype "/" subtype *( ";" parameter )

    The maintype and substype are tokens.  Theoretically they could
    be checked against the official IANA list + x-token, but we
    don't do that.
    """

def parse_content_disposition_header(value: str) -> ContentDisposition:
    """disposition-type *( ";" parameter )"""

def parse_content_transfer_encoding_header(value: str) -> ContentTransferEncoding:
    """mechanism"""
