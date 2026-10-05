"""Shared release-impact and baseline descriptor invariants."""
import re

LEVELS = ("patch", "minor", "major")
SCHEMA = 3
LIBRARY_KINDS = {"lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"}


class CheckError(Exception):
    """An input or analysis failure, distinct from incompatible APIs."""


def valid_identifier(value, *, crate=False):
    pattern = r"[A-Za-z0-9_-]+" if crate else r"[A-Za-z0-9_]+"
    return isinstance(value, str) and re.fullmatch(pattern, value) is not None


def library_target(package):
    return next((target for target in package["targets"]
                 if LIBRARY_KINDS.intersection(target["kind"])), None)


def crate_descriptor(package):
    library = library_target(package)
    return {"name": package["name"], "version": package["version"],
            "library": library is not None,
            "proc_macro": library is not None and "proc-macro" in library["kind"],
            "lib_name": library["name"] if library else None}


def exceeds_intent(required, declared):
    return LEVELS.index(required) > LEVELS.index(declared)


def validate_crate_descriptor(entry):
    if not isinstance(entry, dict) or not all(isinstance(entry.get(flag), bool)
                                             for flag in ("library", "proc_macro")):
        raise CheckError("invalid baseline crate descriptor")
    if not valid_identifier(entry.get("name"), crate=True):
        raise CheckError("invalid baseline crate name")
    if entry["library"] and not valid_identifier(entry.get("lib_name")):
        raise CheckError(f"invalid baseline library target name: {entry['name']}")
    if entry["proc_macro"] and not entry["library"]:
        raise CheckError("proc-macro baseline target must be a library")
