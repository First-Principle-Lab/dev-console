#!/usr/bin/env python3
"""Point the Secret Service "default" alias at the first unlocked collection.

Used only by scripts/test-keyring.sh. gnome-keyring-daemon started outside a real login
session exposes an in-memory "session" collection but keeps its default alias pointing at a
"login" collection that does not exist, and it refuses to create one without a GUI prompter
(gcr-prompter needs a display). The D-Bus call below is the same one a desktop session
performs at login, so afterwards clients see a normal, usable default collection.

Exit 0 -> alias set. Exit 77 -> UNVERIFIED (no provider, no python-dbus, nothing to point at).
"""

import sys

try:
    import dbus
except ImportError:
    print("UNVERIFIED: python3-dbus is not installed, cannot set the default collection alias")
    sys.exit(77)

SERVICE = "org.freedesktop.secrets"
PATH = "/org/freedesktop/secrets"


def main() -> int:
    try:
        bus = dbus.SessionBus()
        service = bus.get_object(SERVICE, PATH)
        interface = dbus.Interface(service, "org.freedesktop.Secret.Service")
        properties = dbus.Interface(service, "org.freedesktop.DBus.Properties")
        collections = [str(c) for c in properties.Get("org.freedesktop.Secret.Service", "Collections")]
    except dbus.DBusException as error:
        print(f"UNVERIFIED: no usable Secret Service provider on this bus ({error.get_dbus_name()})")
        return 77

    if not collections:
        print("UNVERIFIED: the provider registered but exposes no collection")
        return 77

    target = collections[0]
    try:
        collection = bus.get_object(SERVICE, target)
        locked = bool(
            dbus.Interface(collection, "org.freedesktop.DBus.Properties").Get(
                "org.freedesktop.Secret.Collection", "Locked"
            )
        )
    except dbus.DBusException as error:
        print(f"UNVERIFIED: cannot read the collection state ({error.get_dbus_name()})")
        return 77
    if locked:
        print("UNVERIFIED: the only collection is locked and cannot be unlocked headlessly")
        return 77

    interface.SetAlias("default", dbus.ObjectPath(target))
    print(f"default collection alias -> {target}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
