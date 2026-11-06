#!/usr/bin/env python3
"""Render the tracked OpenRPC schemas into a staged mdBook source tree."""

import json
import re
import subprocess
import sys
from pathlib import Path


# These are transport availability facts, not copies of schema definitions.
# Reconcile new declarations against the server dispatcher before publishing.
AVAILABILITY = {
    "echo": True,
    "tokenize_sql": True,
    "initialize_schema_context": True,
    "read_collection": True,
    "release_collection": True,
    "analyze_query": True,
    "list_object": False,
}
BEGIN = "<!-- BEGIN GENERATED SERVER API METHODS -->"
END = "<!-- END GENERATED SERVER API METHODS -->"


def markdown_text(value):
    """Keep schema descriptions as text, rather than interpreting their markup."""
    return re.sub(r"([\\`*_{}\[\]<>])", r"\\\1", value)


def anchor(role, name):
    # Hex encoding avoids punctuation, Unicode, and case-folding collisions.
    return "schema-" + role.encode().hex() + "-" + name.encode().hex()


def refs(value):
    if isinstance(value, dict):
        if "$ref" in value:
            yield value["$ref"]
        for child in value.values():
            yield from refs(child)
    elif isinstance(value, list):
        for child in value:
            yield from refs(child)


def reference_links(value, definitions, role):
    links = []
    for ref in sorted(set(refs(value))):
        prefix = "#/$defs/"
        if not isinstance(ref, str) or not ref.startswith(prefix):
            raise ValueError(f"unsupported schema reference: {ref!r}")
        encoded = ref[len(prefix):]
        # JSON Pointer escapes, not URL slugs. Reject malformed pointers.
        if re.search(r"~(?![01])", encoded) or "/" in encoded:
            raise ValueError(f"unsupported schema reference: {ref}")
        name = encoded.replace("~1", "/").replace("~0", "~")
        if name not in definitions:
            raise ValueError(f"unresolved schema reference: {ref}")
        links.append(f"[{markdown_text(name)}](#{anchor(role, name)})")
    return "Referenced definitions: " + ", ".join(links) + ".\n\n" if links else ""


def json_block(value):
    text = json.dumps(value, ensure_ascii=False, indent=2)
    # Choose a fence that cannot be terminated by data in a schema description.
    runs = [len(run) for run in re.findall(r"`+", text)]
    fence = "`" * max(3, max(runs, default=0) + 1)
    return f"{fence}json\n{text}\n{fence}\n\n"


def render_schema(schema, role):
    if not isinstance(schema, dict):
        return json_block(schema)
    definitions = schema.get("$defs", {})
    if not isinstance(definitions, dict):
        raise ValueError("$defs must be an object")
    root = {key: value for key, value in schema.items() if key != "$defs"}
    output = json_block(root) + reference_links(root, definitions, role)
    for name, definition in sorted(definitions.items()):
        output += f'<a id="{anchor(role, name)}"></a>\n\n'
        output += f"### {markdown_text(name)}\n\n"
        output += json_block(definition) + reference_links(definition, definitions, role)
    return output


def render_method(method, template, workdir, destination):
    contract = method["contract"]
    name = contract["name"]
    if not re.fullmatch(r"[A-Za-z0-9_-]+", name):
        raise ValueError(f"unsafe method name: {name!r}")
    if name not in AVAILABILITY:
        raise ValueError(f"confirm server dispatch before documenting method: {name}")
    schemas = []
    for index, param in enumerate(method["parameters"]):
        descriptor = param["descriptor"]
        label = f"Parameter: {markdown_text(descriptor['name'])}"
        description = "Required." if descriptor.get("required") else "Optional."
        extra = {k: v for k, v in descriptor.items() if k not in ("name", "required")}
        body = (json_block(extra) if extra else "") + render_schema(param["schema"], f"param-{index}")
        schemas.append({"heading": label, "description": description, "body": body})
    result = method.get("result")
    if result is not None:
        descriptor = result["descriptor"]
        extra = {k: v for k, v in descriptor.items() if k != "name"}
        body = (json_block(extra) if extra else "") + render_schema(result["schema"], "result")
        schemas.append({"heading": f"Result: {markdown_text(descriptor['name'])}",
                        "description": "", "body": body})
    metadata = {k: v for k, v in contract.items()
                if k not in ("name", "summary") and v not in (None, [], {})}
    context = {
        "name": markdown_text(name),
        "summary": markdown_text(contract.get("summary") or ""),
        "availability": ("Available after authentication." if AVAILABILITY[name] else
                         "**Unavailable:** this method is declared in OpenRPC but is not dispatched by the server."),
        "metadata": json_block(metadata) if metadata else "",
        "schemas": schemas,
    }
    context_path = workdir / "context.json"
    context_path.write_text(json.dumps(context, ensure_ascii=False), encoding="utf-8")
    with destination.open("w", encoding="utf-8") as output:
        subprocess.run(["tera", "--file", str(template), "--json", str(context_path)],
                       stdout=output, check=True)


def main():
    spec, book, template, workdir = map(Path, sys.argv[1:])
    document = json.loads(spec.read_text(encoding="utf-8"))
    methods = document["methods"]
    names = [method["contract"]["name"] for method in methods]
    if len(names) != len(set(names)) or set(names) != set(AVAILABILITY):
        raise ValueError("OpenRPC method set changed; reconcile it with server dispatch")
    target = book / "api" / "methods"
    target.mkdir(parents=True, exist_ok=False)
    for method in methods:
        render_method(method, template, workdir, target / f"{method['contract']['name']}.md")
    navigation = "\n".join(
        f"- [{name}{'' if AVAILABILITY[name] else ' (unavailable)'}](api/methods/{name}.md)"
        for name in names
    )
    summary = book / "SUMMARY.md"
    text = summary.read_text(encoding="utf-8")
    if text.count(BEGIN) != 1 or text.count(END) != 1:
        raise ValueError("expected one generated-method navigation marker pair")
    before, rest = text.split(BEGIN)
    _, after = rest.split(END)
    summary.write_text(before + BEGIN + "\n" + navigation + "\n" + END + after, encoding="utf-8")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"server API reference: {error}", file=sys.stderr)
        sys.exit(1)
