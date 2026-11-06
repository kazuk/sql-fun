#!/usr/bin/env python3
"""Check links in the rendered Server API chapters, including schema anchors."""

from html.parser import HTMLParser
from pathlib import Path
import sys
from urllib.parse import unquote, urlsplit


class Page(HTMLParser):
    def __init__(self, path):
        super().__init__()
        self.ids = set()
        self.links = []
        self.all_links = []
        self.main_depth = 0
        self.feed(path.read_text(encoding="utf-8"))

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "main":
            self.main_depth += 1
        if "id" in attrs:
            self.ids.add(attrs["id"])
        if tag == "a" and "href" in attrs:
            self.all_links.append(attrs["href"])
            if self.main_depth:
                self.links.append(attrs["href"])

    def handle_endtag(self, tag):
        if tag == "main":
            self.main_depth -= 1


def main():
    root = Path(sys.argv[1]).resolve()
    paths = sorted((root / "api").glob("*.html"))
    paths.extend(sorted((root / "api" / "methods").glob("*.html")))
    if not paths or not (root / "api" / "methods").is_dir():
        raise ValueError("rendered Server API chapters are missing")
    cache = {}

    def page(path):
        if path not in cache:
            cache[path] = Page(path)
        return cache[path]

    expected = {f"api/methods/{path.name}" for path in (root / "api" / "methods").glob("*.html")}
    navigation = [link for link in page(root / "toc.html").all_links if link.startswith("api/methods/")]
    if len(navigation) != len(expected) or set(navigation) != expected:
        raise ValueError("method navigation does not match generated chapters")

    for source in paths:
        for link in page(source).links:
            url = urlsplit(link)
            if url.scheme or url.netloc or link.startswith("//"):
                continue
            target = (source.parent / unquote(url.path)).resolve() if url.path else source
            if not target.is_relative_to(root) or not target.is_file():
                raise ValueError(f"{source.relative_to(root)}: missing link target {link}")
            if url.fragment and target.suffix == ".html":
                if unquote(url.fragment) not in page(target).ids:
                    raise ValueError(f"{source.relative_to(root)}: missing anchor {link}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        print(f"server API links: {error}", file=sys.stderr)
        sys.exit(1)
