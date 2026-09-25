#!/bin/sh
# sql-fun extension export protocol 1. Run on the PostgreSQL server host.
# Usage: sh export-extensions.sh > extensions.tar
# PG_CONFIG may select a particular PostgreSQL installation.
# Requires POSIX sh, pg_config, mktemp, cp, rm, mkdir, and GNU tar.
set -eu
export LC_ALL=C
pg_config=${PG_CONFIG:-pg_config}
sharedir=$("$pg_config" --sharedir)
version=$("$pg_config" --version)
case "$version" in
    'PostgreSQL '*) version=${version#PostgreSQL } ;;
    *) echo 'Unsupported pg_config version output' >&2; exit 1 ;;
esac
# Distribution suffixes do not form part of the PostgreSQL version.
version=${version%% *}
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
trap 'exit 1' HUP INT TERM
mkdir "$stage/extension"
printf '1\n%s\n' "$version" > "$stage/VERSION"
for file in "$sharedir"/extension/*.control "$sharedir"/extension/*.sql; do
    # An unmatched glob is allowed; a dangling symlink is an error.
    if [ ! -e "$file" ] && [ ! -L "$file" ]; then continue; fi
    if [ ! -f "$file" ]; then echo 'Extension source is not a readable regular file' >&2; exit 1; fi
    # Dereference packaging symlinks while exporting; the bundle has only files.
    cp -L -- "$file" "$stage/extension/"
done
tar --format=pax --pax-option=exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime \
    --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner \
    --mode='u+rw,go+r,go-w,a-x' -C "$stage" -cf - VERSION extension
