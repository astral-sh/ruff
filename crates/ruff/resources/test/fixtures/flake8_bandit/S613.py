from ldap.filter import filter_format, escape_filter_chars as efc
from ldap.dn import escape_dn_chars as edc
ldap_base = 'dc=base'
username = 'Administrator'
username_f_escaped = efc(username)
username_dn_escaped = edc(username)
attr = 'uid'
date = 5

# filter: single-line failures
"cn=%s" % username  # FIXME: hmm. probably causes a lot of false-positives. should we?
"(cn=%s)" % username
filter1 = "(&(objectClass=person)(uid=" + username + ")"
filter2 = "(&(objectClass=person)(uid=%s))" % (username,)
filter3 = "(&(objectClass=person)(uid={}))".format(username)
filter4 = f"(&(objectClass=person)(uid={username}))"
"(&(objectClass=person)(" + attr + "=Administrator)"  # hm? better find.
"(  &  (  objectClass  =  person  ) ( uid = " + username + " )"
filter_template = f'(&({attr}={username}){filter1})'
filter_template = f'(&({attr}={{username}}){filter1})'

# OK
"(&(objectClass=person)(uid=Administrator))"
efilter1 = "(&(objectClass=person)(uid=" + efc(username) + "))"
efilter2 = "(&(objectClass=person)(uid=%s))" % (efc(username),)
from ldap3.utils.conv import escape_filter_chars as efc  # noqa: E402
efilter3 = "(&(objectClass=person)(uid={}))".format(efc(username))
efilter3 = "(&(objectClass=person)(uid={username}))".format(username=efc(username))
efilter4 = f"(&(objectClass=person)(uid={efc(username)}))"
ffilter2 = filter_format("(&(objectClass=person)(uid=%s))", (username,))
"(&(objectClass=person)(uid=" + username_f_escaped + "))"  # false positive: we don't make variable inspection  # noqa: E501
"(&(objectClass=person)(uid=Administrator))".format()  # false positive
"(&(objectClass=person)(uid=Administrator))" % {}  # false positive
"(&(objectClass=person)(uid=Administrator))" + efilter1  # false positive

filter_template = f'(&({attr}=%s){filter1})'  # false positive: wrong format specifier matched  # noqa: E501
filter_format(filter_template, (username,))

# prevent false positive:
'--junit-xml=%s' % (username,)
f'{self.__class__.__name__}(level={level!r}, message={message!r})'  # noqa: F821

# DN: single line failures
dn1 = 'uid=' + username + ',cn=users,dc=base'
dn2 = 'uid=%s,cn=users,dc=base' % (username,)
dn3 = 'uid={},cn=users,dc=base'.format(username)
dn4 = f'uid={username},cn=users,dc=base'
dn4 = f'cn=foobar,uid={username},cn=users,dc=example,dc=org'
dn4 = f'uid={username},dc=base'
'uid=%s,cn=users,%s' % (username, ldap_base)
'UID = %s , cn = users , %s' % (username, ldap_base)
'entries=uid=%s,cn=users,%s' % (username, ldap_base)
f'(&(objectClass=posixAccount)(shadowExpire>={date - 1!r})(shadowExpire<={date + 1!r}))'
"dc=%s" % username  # FIXME: hmm. probably causes a lot of false-positives. should we?

# OK
'uid=Administrator,cn=users,dc=base'
dn1 = 'uid=' + edc(username) + ',cn=users,dc=base'
dn2 = 'uid=%s,cn=users,dc=base' % (edc(username),)
dn3 = 'uid={},cn=users,dc=base'.format(edc(username))
dn4 = f'uid={edc(username)},cn=users,dc=base'
'uid=%s,cn=users,%s' % (edc(username), ldap_base)
'UID = %s , cn = users , %s' % (edc(username), ldap_base)
f'cn=users,{ldap_base}'
f'cn=users,{something},dc=base'  # noqa: F821
'cn=users,%(ldap_base)s' % { 'ldap_base': ldap_base }
'%s=Administrator,cn=users,dc=base' % (attr,)
'uid=Administrator,%s=users,dc=base' % (attr,)
'CN=MicrosoftDNS,%s,%s' % ("CN=System" if True else "DC=DomainDnsZones", ldap_base)
'entries=uid=Administrator,cn=users,%s' % (ldap_base,)
f'(&(objectClass=posixAccount)(shadowExpire>={edc(str(date - 1))!r})(shadowExpire<={edc(str(date + 1))!r}))'  # noqa: E501
'uid=' + username_dn_escaped + ',cn=users,dc=base'  # false positive

# unsure / tricky:
not_escaped = 'person'
tbdfilter1 = "(&(objectClass=%s)(uid=%s))" % (not_escaped, efc(username),)  # false negative: only one arg is escaped  # noqa: E501
tbdfilter2 = "(&{fil}(cn={username}))".format(fil=tbdfilter1, username=efc(username))  # false negative: no "(" -> no match  # noqa: E501
'(|(%s)%s)' % (tbdfilter1, tbdfilter2)  # true negative
'(|(uid=%s))' % ')(uid='.join(map(efc, [username, username]))  # true positive
'(|(uid=' + ')(uid='.join({}.keys()) + '))'  # true positive
"cn=" + dn1[4:] if dn1.lower().startswith("uid=") else dn1  # false negative
'cn=%s,%s' % (username, ','.join(dn1.split(',')[1:]))  # TODO: another rule which detects broken parent-DN creation  # noqa: E501

def foo(string):
    return escape_filter_chars(string)  # noqa: F821

def bar(string):
    return escape_dn_chars(string)  # noqa: F821

"(&(objectClass=person)(uid=" + foo(username) + ")"  # false positive: do function inspection  # noqa: E501
'uid=' + bar(username) + ',cn=users,dc=base'  # false negative
