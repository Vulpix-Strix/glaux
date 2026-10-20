"""Generate and verify the third-party license data embedded in Glaux.

Usage examples:
    mise run licenses
    mise run licenses-check

`generate` runs cargo-about and writes apps/glaux/licenses/third-party.json.
`check` regenerates the data in memory and fails when the committed file is
out of date or when extra.json is malformed or repeats a cargo-about entry.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
LICENSES_DIR = REPOSITORY_ROOT / "apps" / "glaux" / "licenses"
THIRD_PARTY = LICENSES_DIR / "third-party.json"
EXTRA = LICENSES_DIR / "extra.json"
CATEGORIES = frozenset({"library", "native", "art"})
TEXT_KINDS = frozenset({"license", "template", "notice"})


class LicenseError(Exception):
    """An expected cargo-about, schema, or consistency failure."""


def run_cargo_about() -> dict:
    command = [
        "cargo",
        "about",
        "generate",
        "--format",
        "json",
        "--locked",
        "--config",
        str(REPOSITORY_ROOT / "about.toml"),
        "--manifest-path",
        str(REPOSITORY_ROOT / "apps" / "glaux" / "Cargo.toml"),
    ]
    result = subprocess.run(command, capture_output=True, text=True, cwd=REPOSITORY_ROOT)
    if result.returncode != 0:
        raise LicenseError(f"cargo about failed:\n{result.stderr}")
    return json.loads(result.stdout)


# Unresolved placeholders left in a license template. The bracketed appendix
# of the Apache license is part of its legal text and is deliberately not a marker.
TEMPLATE_MARKER = re.compile(r"<(?:year|owner|copyright holders?)>", re.IGNORECASE)

# askalono reports a file that carries this exception as plain Apache-2.0.
LLVM_EXCEPTION_MARKER = "LLVM Exceptions to the Apache 2.0 License"


def describe_text(group: dict) -> dict:
    """Builds the embedded text object for one cargo-about license group."""
    text = {
        "kind": "template" if TEMPLATE_MARKER.search(group["text"]) else "license",
        "title": group["name"],
    }
    spdx = group["id"]
    if spdx == "Apache-2.0" and LLVM_EXCEPTION_MARKER in group["text"]:
        spdx = "Apache-2.0 WITH LLVM-exception"
        text["title"] += " with LLVM Exception"
    if spdx:
        text["spdx"] = spdx
    text["content"] = group["text"]
    return text


def normalize(report: dict) -> dict:
    """Reduces cargo-about's report to the compact, deterministic form we embed.

    Texts keep the identification cargo-about made and are stored once, and
    entries refer to them by index. Workspace crates are first-party and are
    left out.
    """
    by_content = {group["text"]: describe_text(group) for group in report["licenses"]}
    texts = sorted(
        by_content.values(),
        key=lambda text: (text.get("spdx", ""), text["title"], text["content"]),
    )
    index_of = {text["content"]: index for index, text in enumerate(texts)}

    text_ids: dict[str, set[int]] = {}
    for group in report["licenses"]:
        for used in group["used_by"]:
            text_ids.setdefault(used["crate"]["id"], set()).add(index_of[group["text"]])

    entries = []
    for crate in report["crates"]:
        package = crate["package"]
        if package["source"] is None:
            continue
        entries.append(
            {
                "name": package["name"],
                "version": package["version"],
                "license": crate["license"],
                "repository": package["repository"],
                "category": "library",
                "text_ids": sorted(text_ids.get(package["id"], ())),
            }
        )
    entries.sort(key=lambda entry: (entry["name"], entry["version"]))
    return {"texts": texts, "entries": entries}


def dump(data: dict) -> str:
    return json.dumps(data, indent=2, ensure_ascii=False) + "\n"


def validate(name: str, data: dict) -> None:
    texts = data.get("texts")
    entries = data.get("entries")
    if not isinstance(texts, list) or not isinstance(entries, list) or not entries:
        raise LicenseError(f"{name}: expected non-empty `texts` and `entries` lists")
    seen = set()
    for index, text in enumerate(texts):
        label = f"{name}: text {index}"
        if text.get("kind") not in TEXT_KINDS:
            raise LicenseError(f"{label} has unknown kind {text.get('kind')!r}")
        for field in ("title", "content"):
            if not isinstance(text.get(field), str) or not text[field].strip():
                raise LicenseError(f"{label} has an empty `{field}`")
        if "spdx" in text and (not isinstance(text["spdx"], str) or not text["spdx"]):
            raise LicenseError(f"{label} has an empty `spdx`")
        if text["content"] in seen:
            raise LicenseError(f"{label} repeats another text")
        seen.add(text["content"])
    for entry in entries:
        label = f"{name}: {entry.get('name')!r}"
        for field in ("name", "version", "license", "repository", "category", "text_ids"):
            if field not in entry:
                raise LicenseError(f"{label} is missing `{field}`")
        if entry["category"] not in CATEGORIES:
            raise LicenseError(f"{label} has unknown category {entry['category']!r}")
        if not entry["text_ids"] or not all(
            isinstance(i, int) and 0 <= i < len(texts) for i in entry["text_ids"]
        ):
            raise LicenseError(f"{label} has missing or out-of-range `text_ids`")


def check_extra_is_disjoint(generated: dict, extra: dict) -> None:
    known = {(e["name"], e["version"]) for e in generated["entries"]}
    for entry in extra["entries"]:
        if (entry["name"], entry["version"]) in known:
            raise LicenseError(
                f"extra.json repeats {entry['name']} {entry['version']}, "
                "which cargo-about already provides"
            )
    provided = {text["content"] for text in generated["texts"]}
    for text in extra["texts"]:
        if text["content"] in provided:
            raise LicenseError(f"extra.json repeats a text cargo-about provides: {text['title']!r}")


def load_extra() -> dict:
    extra = json.loads(EXTRA.read_text(encoding="utf-8"))
    validate("extra.json", extra)
    return extra


def generate() -> int:
    generated = normalize(run_cargo_about())
    validate("third-party.json", generated)
    check_extra_is_disjoint(generated, load_extra())
    THIRD_PARTY.write_text(dump(generated), encoding="utf-8")
    print(f"wrote {THIRD_PARTY.relative_to(REPOSITORY_ROOT)} ({len(generated['entries'])} crates)")
    return 0


def check() -> int:
    generated = normalize(run_cargo_about())
    validate("third-party.json", generated)
    check_extra_is_disjoint(generated, load_extra())
    committed = THIRD_PARTY.read_text(encoding="utf-8") if THIRD_PARTY.exists() else ""
    if committed != dump(generated):
        raise LicenseError("third-party.json is out of date, run `mise run licenses`")
    print("third-party licenses are up to date")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("generate", "check"))
    args = parser.parse_args()
    try:
        return generate() if args.command == "generate" else check()
    except LicenseError as err:
        print(f"error: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
