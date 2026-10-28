#!/usr/bin/env python3
"""Checks the generated glaux.d.luau against a real luau-lsp.

    LUAU_LSP=/path/to/luau-lsp python scripts/luau_lsp_check.py

Not part of `mise run verify`: it needs a luau-lsp binary, which Glaux does
not download or ship. See docs of `mise run luau-lsp-check`.

`luau_lsp/ok.luau` must analyze without errors. In `luau_lsp/bad.luau`
every line ending in `-- error` must report an error and no other line may.
The examples are analyzed too. Errors about the cartridge's own code are
reported but only those that point at a Glaux declaration fail the check.
"""

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFINITIONS = ROOT / "crates/glaux-luau/definitions/glaux.d.luau"
FIXTURES = Path(__file__).resolve().parent / "luau_lsp"
EXAMPLES = ROOT / "examples"
DIAGNOSTIC = re.compile(r"^(?P<file>.+?)\((?P<line>\d+),\d+\): (?P<kind>\w+): (?P<message>.*)$")
GLAUX_API = re.compile(
    r"Unknown global|'(Button|Key|Waveform)'|Function '(log|audio)\.|table '(Button|Key|audio|log)'"
)
FATAL = ("Failed to read definitions", "SyntaxError")


def analyze(binary: str, target: Path) -> tuple[list[tuple[str, int, str]], str]:
    result = subprocess.run(
        [binary, "analyze", f"--definitions={DEFINITIONS}", str(target)],
        capture_output=True,
        text=True,
        check=False,
    )
    output = result.stdout + result.stderr
    errors = []
    for line in output.splitlines():
        match = DIAGNOSTIC.match(line)
        if match and match["kind"] in ("TypeError", "SyntaxError"):
            errors.append((Path(match["file"]).name, int(match["line"]), match["message"]))
    return errors, output


def main() -> int:
    binary = os.environ.get("LUAU_LSP")
    if not binary or not Path(binary).is_file():
        print("set LUAU_LSP to a luau-lsp executable (tested with 1.70.1)", file=sys.stderr)
        return 2
    failures = []

    errors, output = analyze(binary, FIXTURES / "ok.luau")
    if any(text in output for text in FATAL):
        failures.append(f"the definitions did not load:\n{output}")
    failures += [f"ok.luau:{line}: unexpected error: {message}" for _, line, message in errors]

    errors, output = analyze(binary, FIXTURES / "bad.luau")
    expected = {
        number
        for number, text in enumerate((FIXTURES / "bad.luau").read_text().splitlines(), 1)
        if text.rstrip().endswith("-- error")
    }
    reported = {line for _, line, _ in errors}
    for line in sorted(expected - reported):
        failures.append(f"bad.luau:{line}: expected an error, got none")
    for _, line, message in errors:
        if line not in expected:
            failures.append(f"bad.luau:{line}: unexpected error: {message}")

    for code in sorted(EXAMPLES.glob("*/code")):
        errors, _ = analyze(binary, code)
        for name, line, message in errors:
            report = f"{code.parent.name}/{name}:{line}: {message}"
            if GLAUX_API.search(message):
                failures.append(report)
            else:
                print(f"note: {report}", file=sys.stderr)

    for failure in failures:
        print(failure, file=sys.stderr)
    print(f"luau-lsp-check {'failed' if failures else 'ok'}: {len(failures)} problem(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
