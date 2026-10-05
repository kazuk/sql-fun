"""Explicit repository feature policy and baseline coverage contracts."""
import json
import tomllib

from . import commands

from .model import CheckError, valid_identifier


def validate_feature_sets(feature_sets, available):
    """Require complete publishable-crate mappings and existing Cargo features."""
    if not isinstance(feature_sets, dict) or not feature_sets:
        raise CheckError("feature-set must be a nonempty table")
    normalized = {}
    for name, mapping in sorted(feature_sets.items()):
        if not valid_identifier(name, crate=True) or not isinstance(mapping, dict):
            raise CheckError("invalid feature-set name or mapping")
        missing, unknown = available.keys() - mapping.keys(), mapping.keys() - available.keys()
        if missing or unknown:
            raise CheckError(f"feature-set {name}: missing crate mappings {sorted(missing)}; "
                             f"non-publishable or unknown crates {sorted(unknown)}")
        normalized[name] = {}
        for crate, selected in sorted(mapping.items()):
            if not isinstance(selected, list) or any(not isinstance(value, str) for value in selected):
                raise CheckError(f"feature-set {name}/{crate}: features must be a list of names")
            unknown_features = set(selected) - set(available[crate])
            if unknown_features:
                raise CheckError(f"feature-set {name}/{crate}: unknown features {sorted(unknown_features)}")
            normalized[name][crate] = sorted(set(selected))
    return normalized


def load_feature_sets(path, packages):
    """Read only the explicitly selected policy file."""
    with path.open("rb") as configuration:
        data = tomllib.load(configuration)
    if set(data) != {"feature-set"}:
        raise CheckError("feature policy must contain only the feature-set table")
    return validate_feature_sets(data["feature-set"],
                                 {package["name"]: package["features"] for package in packages})


def validate_baseline_feature_sets(root, manifest):
    metadata = json.loads(commands.release_metadata(root / "packages"))
    members = set(metadata["workspace_members"])
    available = {package["name"]: package["features"] for package in metadata["packages"]
                 if package["id"] in members}
    names = {entry["name"] for entry in manifest["crates"]}
    if available.keys() != names:
        raise CheckError("baseline packaged workspace differs from crate descriptors")
    return validate_feature_sets(manifest.get("feature_sets"), available)


def require_matching_coverage(previous, requested):
    """New crates have no previous APIs; existing mappings must match exactly."""
    if previous.keys() != requested.keys():
        raise CheckError("baseline feature-set identities differ; explicit baseline transition required")
    for name, mapping in previous.items():
        for crate, selected in mapping.items():
            if crate in requested[name] and requested[name][crate] != selected:
                raise CheckError(f"baseline feature-set {name}/{crate} differs; explicit baseline transition required")
