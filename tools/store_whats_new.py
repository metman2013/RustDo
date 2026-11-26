#!/usr/bin/env python3
"""Puts a version's CHANGELOG notes into a Microsoft Store submission's "What's new".

Used by the store-submit job in .github/workflows/build.yml, between uploading the
package to a draft submission and sending it for certification:

    store_whats_new.py notes <version>                 # print the Store text (for the log)
    store_whats_new.py patch <version> <in> <out>      # submission JSON in -> JSON out

`patch` reads the JSON that `msstore submission get` printed (the tool's status lines
around it are skipped), sets `releaseNotes` in each English listing (every listing, if
there's no English one), and writes the result for `msstore submission updateMetadata`.
It exits with 1 if the JSON can't be found or has no listings; the job then submits
without changing "What's new".
"""

import json
import re
import sys
from pathlib import Path

STORE_LIMIT = 1500
ROOT = Path(__file__).resolve().parent.parent


def notes(version: str, changelog: str) -> str:
    """The version's CHANGELOG section as plain Store text: headings, then "• " lines."""
    lines, on = [], False
    for line in changelog.splitlines():
        if line.startswith("## "):
            on = line == f"## {version}" or line.startswith(f"## {version} ")
            continue
        if not on:
            continue
        if line.startswith("### "):
            if lines:
                lines.append("")
            lines.append(line[4:].strip())
        elif line.startswith("- "):
            lines.append("• " + line[2:].strip())
    text = "\n".join(lines).strip()
    if len(text) <= STORE_LIMIT:
        return text
    # Too long: keep whole lines, and say there's more.
    more = "\n• …and more: see the release notes on GitHub."
    kept = []
    for line in text.split("\n"):
        if len("\n".join(kept + [line])) + len(more) > STORE_LIMIT:
            break
        kept.append(line)
    return "\n".join(kept).rstrip() + more


def find_json(output: str):
    """The JSON object in the tool's output, skipping status lines before and after it."""
    start = output.find("{")
    while start != -1:
        try:
            value, _ = json.JSONDecoder().raw_decode(output[start:])
            if isinstance(value, dict):
                return value
        except ValueError:
            pass
        start = output.find("{", start + 1)
    return None


def key(obj: dict, name: str):
    """The key spelled however the tool spells it (listings / Listings)."""
    return next((k for k in obj if k.lower() == name.lower()), None)


def patch(submission: dict, text: str) -> int:
    """Sets the release notes; returns how many listings were changed."""
    listings_key = key(submission, "listings")
    listings = submission.get(listings_key) if listings_key else None
    if not isinstance(listings, dict) or not listings:
        return 0
    english = [lang for lang in listings if lang.lower().startswith("en")]
    changed = 0
    for lang in english or list(listings):
        listing = listings[lang]
        if not isinstance(listing, dict):
            continue
        base_key = key(listing, "baseListing") or "baseListing"
        base = listing.setdefault(base_key, {})
        if not isinstance(base, dict):
            continue
        notes_key = key(base, "releaseNotes") or ("ReleaseNotes" if base_key[0].isupper() else "releaseNotes")
        base[notes_key] = text
        changed += 1
    return changed


def main(argv):
    if len(argv) >= 2 and argv[0] == "notes":
        print(notes(argv[1], (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")))
        return 0
    if len(argv) == 4 and argv[0] == "patch":
        version, src, dst = argv[1], Path(argv[2]), Path(argv[3])
        text = notes(version, (ROOT / "CHANGELOG.md").read_text(encoding="utf-8"))
        if not text:
            print(f"no CHANGELOG notes for {version}", file=sys.stderr)
            return 1
        submission = find_json(src.read_text(encoding="utf-8", errors="replace"))
        if submission is None:
            print("no submission JSON in the tool's output", file=sys.stderr)
            return 1
        changed = patch(submission, text)
        if not changed:
            print("the submission has no listings to update", file=sys.stderr)
            return 1
        dst.write_text(json.dumps(submission, ensure_ascii=False), encoding="utf-8")
        print(f"What's new set in {changed} listing(s)")
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
