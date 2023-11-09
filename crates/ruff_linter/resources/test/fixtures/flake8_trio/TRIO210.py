import requests
import httpx
import urllib3
import urllib.request
import urllib.request as request
from urllib.request import urlopen


async def foo():
    requests.get("https://example.com")

async def foo():
    httpx.get("https://example.com")

async def foo():
    urllib3.request("GET", "https://example.com")

async def foo():
    urllib.request.urlopen("https://example.com")

async def foo():
    request.urlopen("https://example.com")

async def foo():
    urlopen("https://example.com")

def foo():
    requests.get("https://example.com")

def foo():
    httpx.get("https://example.com")

def foo():
    urllib3.request("GET", "https://example.com")

def foo():
    urllib.request.urlopen("https://example.com")

def foo():
    request.urlopen("https://example.com")

def foo():
    urlopen("https://example.com")

