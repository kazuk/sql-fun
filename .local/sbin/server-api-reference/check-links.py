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
        self.main_depth = 0
        self.feed(path.read_text(encoding="utf-8"))

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "main":
            self.main_depth += 1
        if "id" in attrs:
            self.ids.add(attrs["id"])
        if tag == "a" and "href" in attrs:
            if self.main_depth:
                self.links.append(attrs["href"])

    def handle_endtag(self, tag):
        if tag == "main":
            self.main_depth -= 1


class Navigation(HTMLParser):
    """Record the chapter ancestors from mdBook's rendered nested list."""

    def __init__(self, path):
        super().__init__()
        self.chapters = []
        self.entries = {}
        self.feed(path.read_text(encoding="utf-8"))

    def handle_starttag(self, tag, attrs):
        if tag == "ol":
            self.chapters.append(None)
        elif tag == "li" and self.chapters:
            self.chapters[-1] = None
        elif tag == "a" and self.chapters:
            href = dict(attrs).get("href")
            if href:
                self.chapters[-1] = href
                self.entries.setdefault(href, []).append(tuple(self.chapters[:-1]))

    def handle_endtag(self, tag):
        if tag == "ol":
            self.chapters.pop()


def check_navigation(root, paths):
    navigation = Navigation(root / "toc.html").entries
    parent = "api/server-api.html"
    methods = "api/methods.html"
    expected = {
        parent: (),
        "api/initialize.html": (parent,),
        "api/check-analysis-design.html": (parent,),
        "api/cli-api.html": (parent,),
        methods: (parent,),
    }
    expected.update({f"api/methods/{path.name}": (parent, methods) for path in paths})
    for href, ancestors in expected.items():
        if navigation.get(href) != [ancestors]:
            raise ValueError(f"incorrect Server API navigation hierarchy: {href}")
    actual_methods = {href for href in navigation if href.startswith("api/methods/")}
    if actual_methods != {href for href in expected if href.startswith("api/methods/")}:
        raise ValueError("method navigation does not match generated chapters")


def main():
    root = Path(sys.argv[1]).resolve()
    method_paths = sorted((root / "api" / "methods").glob("*.html"))
    paths = sorted((root / "api").glob("*.html")) + method_paths
    if not method_paths:
        raise ValueError("rendered Server API chapters are missing")
    cache = {}

    def page(path):
        if path not in cache:
            cache[path] = Page(path)
        return cache[path]

    check_navigation(root, method_paths)

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
