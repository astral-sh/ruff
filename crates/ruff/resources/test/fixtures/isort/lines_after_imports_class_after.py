from __future__ import annotations

from typing import Any

from requests import Session

from my_first_party import my_first_party_object

from . import my_local_folder_object
class Thing(object):
  name: str
  def __init__(self, name: str):
    self.name = name
