# Issue: https://github.com/astral-sh/ruff/issues/20288
import gi

gi.require_version('Gtk', '4.0')
gi.require_versions({'GObject': '2.0', 'Gio': '2.0'})

from gi.repository import GObject, Gio, Gtk
