import ftplib

ftplib.FTP()
ftplib.FTP_TLS()

# Non-function members should not be flagged (https://github.com/astral-sh/ruff/issues/16673)
port = ftplib.FTP_PORT
crlf = ftplib.CRLF

# References in callback positions
map(ftplib.FTP, [])
foo = ftplib.FTP

from ftplib import FTP

FTP()

map(FTP, [])
foo = FTP
