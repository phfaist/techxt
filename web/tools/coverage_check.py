#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Report which codepoints of techxt's own output each display face is missing.

web/PLAN.md §8.5.  With whole fonts (§8.4) and the fallback chain of §8.2 a gap is a
cosmetic mix of faces rather than a box, so this is not a safety net: it is the
evidence behind §8.1's selection criterion, and the one hard gate on the default face.

Usage::

    python3 web/tools/coverage_check.py            # the report, on stdout
    python3 web/tools/coverage_check.py --check    # the CI gate (§11 step 5)
    python3 web/tools/coverage_check.py --keep DIR # also leave the generated document

What it does
------------

1. Builds an "everything" document: every macro name in
   `rust/techxt/src/defs/symbols_extra_data.rs`, every accent in `accents_data.rs`,
   every font-style macro of `defs/fontstyles.rs` over the whole Latin alphabet and
   the digits (plus the nestings that reach the composite alphabets — bold italic,
   bold fraktur, sans-serif bold italic — which no single macro produces), the
   sub/superscripts, a matrix, a table and a nested list.
2. Converts it with the built CLI, `cargo run -q --bin techxt`, at its **default**
   options: what this measures is the out-of-the-box rendering.
3. Collects the codepoints of the output and asks each face's `cmap` about them.

`--check` **fails** when the default face — JuliaMono, `DEFAULT_FONT` in
`src/fonts.ts` — is missing any of them, and **warns** for the other four, writing
their gap lists to ``$GITHUB_STEP_SUMMARY`` when that variable is set and to stdout
otherwise.  A handful of gaps filled by the fallback chain is a documented fact about
a face, not a bug; only a face with a large or ugly gap comes off the list of §8.1.

The default face has exactly two gaps today, named and justified one by one in
``KNOWN_DEFAULT_GAPS``, and the gate is on anything *beyond* them — which is what "not
allowed to regress" means.  A gate that is red on the day it is written is a gate
people learn to ignore; the alternative to a two-line recorded baseline is either no
gate at all, or a default face chosen for a pair of brackets nobody types.

The generated document is written to a temporary directory and deleted, because it is
derived from files already in the repository and committing it would only create a
second place for the same facts to rot.  ``--keep`` puts a copy somewhere for reading.

Reading generated Rust
----------------------

The two data files are generated tables (`tools/gen_symbols.py`), and rustfmt wraps a
long row across several lines, so they cannot be read line by line.  They are read
with a brace-counting scan over the array literal instead, and every reader here
refuses to return an implausibly small table: a parser that silently finds zero macros
would turn this whole check into a test that passes because it tested nothing.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unicodedata
from pathlib import Path

WEB = Path(__file__).resolve().parent.parent
ROOT = WEB.parent
DEFS = ROOT / "rust" / "techxt" / "src" / "defs"
FONTS = WEB / "fonts"

#: The face `src/fonts.ts` names as `DEFAULT_FONT`, and the only hard gate here.
DEFAULT_FACE = "JuliaMono-Regular.woff2"

#: Floors below which a parse is treated as broken rather than as a small table.
#: The tables held 991 symbols and 26 accents when this was written; these are not
#: exact so that adding or dropping an entry does not fail the build, but a parser
#: that has stopped matching the file's shape falls straight through them.
MIN_SYMBOLS = 700
MIN_ACCENTS = 20

#: What every alphabet macro is applied to.  The whole alphabet rather than a sample:
#: the Mathematical Alphanumeric Symbols block has holes patched from Letterlike
#: Symbols — ℎ, ℬ, ℭ, ℕ — and only the full run reaches them.
ALPHABET = (
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz 0123456789"
)

#: The text- and math-mode font-style macros of `defs/fontstyles.rs`.  Listed rather
#: than parsed: they are written as two literal arrays in ordinary Rust code, and a
#: scraper for that would be more fragile than the list going stale, which the
#: assertion below catches anyway.
TEXT_STYLES = (
    "textrm", "textup", "textmd", "textnormal", "textbf", "textit",
    "textsl", "textsf", "texttt", "textsc", "emph",
)
MATH_STYLES = (
    "mathrm", "mathnormal", "mathbf", "mathit", "mathsf", "mathbb",
    "mathtt", "mathcal", "mathscr", "mathfrak",
)
#: The alphabets no single macro selects (`FontStyleKind` has fourteen members).
NESTED_STYLES = (
    r"\mathbf{\mathit{%s}}",
    r"\mathbf{\mathcal{%s}}",
    r"\mathbf{\mathfrak{%s}}",
    r"\mathsf{\mathbf{%s}}",
    r"\mathsf{\mathit{%s}}",
    r"\mathsf{\mathbf{\mathit{%s}}}",
    r"\textbf{\textit{%s}}",
    r"\textsf{\textbf{%s}}",
)


# ------------------------------------------------------------- reading generated Rust


def array_items(source: Path, name: str) -> list[str]:
    """The top-level ``(…)`` items of the Rust array literal assigned to ``name``.

    A scan rather than a line regex, because rustfmt wraps a row with several
    arguments across four lines.  String and char literals are tracked so that a
    parenthesis inside ``Literal("(")`` cannot be mistaken for structure — the symbol
    table contains exactly that.
    """
    text = source.read_text(encoding="utf-8")
    header = re.search(rf"static\s+{re.escape(name)}\s*:[^=]*=\s*&\[", text)
    if header is None:
        raise SystemExit(f"{source}: no `static {name}: … = &[` found")

    items: list[str] = []
    depth = 0
    start = 0
    in_string = in_char = escaped = False
    for index in range(header.end(), len(text)):
        character = text[index]
        if in_string or in_char:
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == '"' and in_string:
                in_string = False
            elif character == "'" and in_char:
                in_char = False
            continue
        if character == '"':
            in_string = True
        elif character == "'":
            in_char = True
        elif character == "(":
            if depth == 0:
                start = index
            depth += 1
        elif character == ")":
            depth -= 1
            if depth == 0:
                items.append(text[start : index + 1])
        elif character == "]" and depth == 0:
            break
    return items


def unescape(literal: str) -> str:
    """A Rust string literal's value: the escapes `gen_symbols.py` actually emits."""
    out: list[str] = []
    index = 0
    while index < len(literal):
        character = literal[index]
        if character != "\\":
            out.append(character)
            index += 1
            continue
        marker = literal[index + 1]
        if marker == "u":
            end = literal.index("}", index)
            out.append(chr(int(literal[index + 3 : end], 16)))
            index = end + 1
        else:
            out.append({"n": "\n", "t": "\t", "r": "\r", "0": "\0"}.get(marker, marker))
            index += 2
    return "".join(out)


def read_symbols() -> list[tuple[str, int, str]]:
    """`(macro name, mandatory-argument count, mode)` for every generated symbol."""
    rows = []
    for item in array_items(DEFS / "symbols_extra_data.rs", "SYMBOLS"):
        name = re.match(r'\(\s*"((?:[^"\\]|\\.)*)"', item)
        mode = re.search(r"(Any|Math|Text)\s*,?\s*\)\s*$", item.replace("\n", " "))
        if name is None or mode is None:
            raise SystemExit(f"symbols_extra_data.rs: cannot read a row: {item[:80]!r}")
        arguments = len(re.findall(r'\(\s*"m"\s*,\s*"[^"]*"\s*\)', item))
        rows.append((unescape(name.group(1)), arguments, mode.group(1)))
    if len(rows) < MIN_SYMBOLS:
        raise SystemExit(
            f"symbols_extra_data.rs: only {len(rows)} macros found, expected at least "
            f"{MIN_SYMBOLS} — the file's shape changed and this parser has not"
        )
    return rows


def read_accents() -> list[str]:
    """Every accent macro name of `accents_data.rs`."""
    names = []
    for item in array_items(DEFS / "accents_data.rs", "ACCENTS"):
        name = re.match(r'\(\s*"((?:[^"\\]|\\.)*)"', item)
        if name is None:
            raise SystemExit(f"accents_data.rs: cannot read a row: {item[:80]!r}")
        names.append(unescape(name.group(1)))
    if len(names) < MIN_ACCENTS:
        raise SystemExit(
            f"accents_data.rs: only {len(names)} accents found, expected at least "
            f"{MIN_ACCENTS} — the file's shape changed and this parser has not"
        )
    return names


def check_styles() -> None:
    """Fail if `fontstyles.rs` grew a macro the lists above do not know about."""
    source = (DEFS / "fontstyles.rs").read_text(encoding="utf-8")
    declared = set(re.findall(r'\("((?:text|math)[a-z]+)",\s*FontStyleKind::', source))
    missing = declared - set(TEXT_STYLES) - set(MATH_STYLES)
    if missing:
        raise SystemExit(
            "defs/fontstyles.rs declares font-style macros this script does not use: "
            + ", ".join(sorted(missing))
            + " — add them to TEXT_STYLES / MATH_STYLES"
        )


# ------------------------------------------------------------------- the document


def macro_call(name: str, arguments: int) -> str:
    """``\\name`` with a filler for each mandatory argument.

    The two control-whitespace macros are written out rather than interpolated: a
    backslash followed by a newline *is* the macro whose name is a newline, and the
    line it ends has to be the one this returns.
    """
    if name == "\n":
        return "\\\n"
    if name == " ":
        return "\\ "
    return "\\" + name + "{x}" * arguments


def build_document() -> str:
    """The everything-document, in a fixed order so two runs produce the same bytes."""
    check_styles()
    symbols = read_symbols()
    accents = read_accents()

    lines = [
        "% GENERATED by web/tools/coverage_check.py (web/PLAN.md §8.5) — not committed.",
        "% Every macro techxt knows, once, so that the codepoints of the conversion can",
        "% be compared against each display face's cmap.",
        "",
        r"\section{Text-mode symbols}",
        "",
    ]

    text_mode = [row for row in symbols if row[2] in ("Text", "Any")]
    math_mode = [row for row in symbols if row[2] == "Math"]

    for name, arguments, _ in text_mode:
        lines.append(macro_call(name, arguments))
    lines += ["", r"\section{Math-mode symbols}", ""]
    for name, arguments, _ in math_mode:
        # Each in its own inline formula: a math-mode macro reached from text mode
        # renders through a different path (see defs/symbols_extra.rs), and it is the
        # math one this document is for.
        lines.append("$" + macro_call(name, arguments) + "$")

    lines += ["", r"\section{Accents}", ""]
    for name in accents:
        for base in ("a", "A", "e", "o", "n", "u", "y"):
            lines.append("\\" + name + "{" + base + "}")
        # An accent with an empty argument renders as the mark's spacing clone, which
        # is a different codepoint again (PLAN.md §9.3).
        lines.append("\\" + name + "{}")

    lines += ["", r"\section{Font styles}", ""]
    for name in TEXT_STYLES:
        lines.append("\\" + name + "{" + ALPHABET + "}")
    for name in MATH_STYLES:
        lines.append("$\\" + name + "{" + ALPHABET + "}$")
    for pattern in NESTED_STYLES:
        body = pattern % ALPHABET
        lines.append(body if body.startswith(r"\text") else "$" + body + "$")
    lines.append("$" + ALPHABET + "$")  # math italic, the default alphabet

    lines += [
        "",
        r"\section{Sub- and superscripts}",
        "",
        r"$x^{0123456789} x_{0123456789}$",
        r"$x^{+-=()n i j} x_{+-=()a e o x h k l m n p s t}$",
        r"$\sum_{i=1}^{n} x_i$, $\int_0^\infty e^{-x^2}\,dx$, $\frac{a+b}{c-d}$,"
        r" $\sqrt{2}$, $\binom{n}{k}$",
        "",
        r"\section{Delimiters and matrices}",
        "",
        r"\begin{pmatrix} a & b & c \\ d & e & f \\ g & h & i \end{pmatrix}",
        "",
        r"\begin{bmatrix} a & b \\ c & d \end{bmatrix}",
        "",
        r"\begin{vmatrix} a & b \\ c & d \end{vmatrix}",
        "",
        r"\begin{Bmatrix} a & b \\ c & d \end{Bmatrix}",
        "",
        r"\begin{cases} a & x > 0 \\ b & x \le 0 \end{cases}",
        "",
        r"$\left( \frac{a}{b} \right) \left[ x \right] \left\{ y \right\}"
        r" \left| z \right| \left\langle w \right\rangle$",
        "",
        r"\section{A table}",
        "",
        r"\begin{tabular}{|l|c|r|}",
        r"\hline",
        r"left & centre & right \\",
        r"\hline",
        r"$\alpha$ & $\beta$ & $\gamma$ \\",
        r"one & two & three \\",
        r"\hline",
        r"\end{tabular}",
        "",
        r"\section{Nested lists}",
        "",
        r"\begin{itemize}",
        r"\item one",
        r"\begin{itemize}",
        r"\item two",
        r"\begin{itemize}",
        r"\item three",
        r"\begin{itemize}",
        r"\item four",
        r"\end{itemize}",
        r"\end{itemize}",
        r"\end{itemize}",
        r"\end{itemize}",
        "",
        r"\begin{enumerate}",
        r"\item one",
        r"\begin{enumerate}",
        r"\item two",
        r"\begin{enumerate}",
        r"\item three",
        r"\begin{enumerate}",
        r"\item four",
        r"\end{enumerate}",
        r"\end{enumerate}",
        r"\end{enumerate}",
        r"\end{enumerate}",
        "",
    ]
    return "\n".join(lines) + "\n"


# --------------------------------------------------------------------- the conversion


def convert(document: Path) -> str:
    """Run the built CLI over ``document`` and return what it printed.

    Exit code 1 means "converted, and the diagnostics contain errors", which this
    document produces on purpose — it calls `\\input` with a filename that does not
    exist, among other things.  Only exit code 2, "could not be converted at all", is
    a failure here.
    """
    if shutil.which("cargo") is None:
        raise SystemExit("cargo is not on PATH; this check converts with the built CLI")
    process = subprocess.run(
        ["cargo", "run", "-q", "--bin", "techxt", "--", "-q", str(document)],
        cwd=ROOT / "rust",
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if process.returncode >= 2:
        sys.stderr.write(process.stderr)
        raise SystemExit(f"techxt exited {process.returncode}: the document did not convert")
    if not process.stdout.strip():
        sys.stderr.write(process.stderr)
        raise SystemExit("techxt produced no output; refusing to report a vacuous pass")
    return process.stdout


#: Unicode general categories no font is asked about.  A control (`Cc`), format
#: (`Cf`) or line/paragraph separator (`Zl`, `Zp`) character is an instruction to the
#: layout, not a glyph: the shaper never draws one, so a face that has no entry for it
#: cannot produce a box.  techxt emits three of them — SOFT HYPHEN from `\-`, ZERO
#: WIDTH NON-JOINER and WORD JOINER — and JuliaMono, like most text faces, maps none
#: of them.  Ordinary spaces (`Zs`) are *not* excluded: a face is expected to have an
#: advance for the spaces the layout engine puts in a line.
INVISIBLE_CATEGORIES = frozenset(("Cc", "Cf", "Zl", "Zp"))

#: The gaps the default face is known and accepted to have, so that this check gates
#: on *new* ones — which is what "not allowed to regress" means in §8.5.
#:
#: U+301A and U+301B are what `\openbracketleft` and `\openbracketright` render as:
#: pylatexenc maps them into CJK Symbols and Punctuation rather than to the
#: mathematical U+27E6 ⟦ and U+27E7 ⟧, which JuliaMono does have.  Two macros nobody
#: writes, rendering to two characters a monospace face reasonably leaves to a CJK
#: font, and the chain of §8.2 supplies them.  Every entry here has to earn its place
#: in a sentence like that one; the list is not a place to put a real gap.
KNOWN_DEFAULT_GAPS = frozenset({0x301A, 0x301B})


def codepoints(text: str) -> set[int]:
    return {
        ord(character)
        for character in text
        if unicodedata.category(character) not in INVISIBLE_CATEGORIES
    }


# ------------------------------------------------------------------------- reporting


def face_cmap(path: Path) -> set[int]:
    from fontTools.ttLib import TTFont

    return set(TTFont(path, lazy=True).getBestCmap())


def describe(code: int) -> str:
    return f"U+{code:04X} {chr(code)!r} {unicodedata.name(chr(code), '<unnamed>')}"


#: Enough of the Unicode block table to summarise a gap list by where it falls, which
#: is the difference between "this face is missing 515 characters" and "this face has
#: no Cyrillic".  Only the blocks techxt's output actually reaches are listed; anything
#: else is reported by its range.
BLOCKS: tuple[tuple[int, int, str], ...] = (
    (0x0000, 0x007F, "Basic Latin"),
    (0x0080, 0x00FF, "Latin-1 Supplement"),
    (0x0100, 0x017F, "Latin Extended-A"),
    (0x0180, 0x024F, "Latin Extended-B"),
    (0x0250, 0x02AF, "IPA Extensions"),
    (0x02B0, 0x02FF, "Spacing Modifier Letters"),
    (0x0300, 0x036F, "Combining Diacritical Marks"),
    (0x0370, 0x03FF, "Greek and Coptic"),
    (0x0400, 0x052F, "Cyrillic"),
    (0x0590, 0x05FF, "Hebrew"),
    (0x0E00, 0x0E7F, "Thai"),
    (0x1D00, 0x1D7F, "Phonetic Extensions"),
    (0x1E00, 0x1EFF, "Latin Extended Additional"),
    (0x2000, 0x206F, "General Punctuation"),
    (0x2070, 0x209F, "Superscripts and Subscripts"),
    (0x20A0, 0x20CF, "Currency Symbols"),
    (0x20D0, 0x20FF, "Combining Diacritical Marks for Symbols"),
    (0x2100, 0x214F, "Letterlike Symbols"),
    (0x2150, 0x218F, "Number Forms"),
    (0x2190, 0x21FF, "Arrows"),
    (0x2200, 0x22FF, "Mathematical Operators"),
    (0x2300, 0x23FF, "Miscellaneous Technical"),
    (0x2400, 0x243F, "Control Pictures"),
    (0x2500, 0x257F, "Box Drawing"),
    (0x25A0, 0x25FF, "Geometric Shapes"),
    (0x2600, 0x26FF, "Miscellaneous Symbols"),
    (0x2700, 0x27BF, "Dingbats"),
    (0x27C0, 0x27EF, "Misc. Mathematical Symbols-A"),
    (0x27F0, 0x27FF, "Supplemental Arrows-A"),
    (0x2900, 0x297F, "Supplemental Arrows-B"),
    (0x2980, 0x29FF, "Misc. Mathematical Symbols-B"),
    (0x2A00, 0x2AFF, "Supplemental Mathematical Operators"),
    (0x2B00, 0x2BFF, "Miscellaneous Symbols and Arrows"),
    (0x3000, 0x303F, "CJK Symbols and Punctuation"),
    (0x1D400, 0x1D7FF, "Mathematical Alphanumeric Symbols"),
    (0x1F300, 0x1F5FF, "Misc. Symbols and Pictographs"),
)


def block_of(code: int) -> str:
    for low, high, name in BLOCKS:
        if low <= code <= high:
            return name
    return f"U+{code & ~0xFF:04X}…"


def histogram(missing: set[int]) -> list[str]:
    """`block: count` for a gap list, largest first."""
    counts: dict[str, int] = {}
    for code in missing:
        counts[block_of(code)] = counts.get(block_of(code), 0) + 1
    return [
        f"{count:>5}  {name}"
        for name, count in sorted(counts.items(), key=lambda pair: (-pair[1], pair[0]))
    ]


def render_gaps(missing: set[int], limit: int = 60) -> list[str]:
    lines = [describe(code) for code in sorted(missing)[:limit]]
    if len(missing) > limit:
        lines.append(f"… and {len(missing) - limit} more")
    return lines


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail on a gap in the default face; warn on the others (CI)",
    )
    parser.add_argument(
        "--keep",
        type=Path,
        default=None,
        help="also write the generated document here, for reading",
    )
    args = parser.parse_args(argv)

    faces = sorted(FONTS.glob("*.woff2"))
    if not faces:
        raise SystemExit(f"{FONTS}: no woff2 faces; run web/tools/fetch_fonts.py")
    if not (FONTS / DEFAULT_FACE).exists():
        raise SystemExit(f"{FONTS / DEFAULT_FACE}: the default face is missing")

    document = build_document()
    with tempfile.TemporaryDirectory(prefix="techxt-coverage-") as scratch:
        path = Path(scratch) / "everything.tex"
        path.write_text(document, encoding="utf-8")
        if args.keep:
            args.keep.mkdir(parents=True, exist_ok=True)
            (args.keep / "everything.tex").write_text(document, encoding="utf-8")
        output = convert(path)
        if args.keep:
            (args.keep / "everything.txt").write_text(output, encoding="utf-8")

    wanted = codepoints(output)
    print(
        f"document: {len(document)} bytes in, {len(output)} bytes out, "
        f"{len(wanted)} distinct codepoints to cover\n"
    )

    summary: list[str] = []
    regressions: set[int] = set()
    stale: set[int] = set()
    for face in faces:
        missing = wanted - face_cmap(face)
        is_default = face.name == DEFAULT_FACE
        role = "default" if is_default else "alternative"
        covered = len(wanted) - len(missing)
        print(f"{face.name} ({role}): {covered}/{len(wanted)} covered, {len(missing)} missing")
        if is_default:
            regressions = missing - KNOWN_DEFAULT_GAPS
            stale = KNOWN_DEFAULT_GAPS - missing
            for code in sorted(missing & KNOWN_DEFAULT_GAPS):
                print(f"    {describe(code)}  (recorded gap, accepted)")
            for line in render_gaps(regressions):
                print(f"    {line}  ** NEW **")
        else:
            for line in histogram(missing):
                print(f"    {line}")
        if missing and not is_default:
            summary.append(f"### {face.name} — {len(missing)} codepoints missing\n")
            summary.append("The chain of §8.2 renders these from the platform's fonts.\n")
            summary.append("```")
            summary.extend(histogram(missing))
            summary.append("")
            summary.extend(render_gaps(missing, limit=200))
            summary.append("```\n")

    if summary:
        step_summary = os.environ.get("GITHUB_STEP_SUMMARY")
        text = "## Display-font coverage gaps (web/PLAN.md §8.5)\n\n" + "\n".join(summary)
        if step_summary:
            with open(step_summary, "a", encoding="utf-8") as handle:
                handle.write(text + "\n")
            print("\ngap lists written to $GITHUB_STEP_SUMMARY")
        else:
            print("\n" + text)

    if stale:
        # Not a failure: a gap that closed is good news, and the only cost of noticing
        # it late is a stale comment.
        print(
            f"\n{DEFAULT_FACE}: {len(stale)} recorded gap(s) have closed — drop them "
            "from KNOWN_DEFAULT_GAPS: " + ", ".join(f"U+{code:04X}" for code in sorted(stale))
        )
    if regressions:
        print(
            f"\n{DEFAULT_FACE} is missing {len(regressions)} codepoint(s) that techxt's "
            "own output contains and that are not in KNOWN_DEFAULT_GAPS. The default "
            "face is the out-of-the-box rendering and may not regress (web/PLAN.md "
            "§8.5): "
            + ", ".join(f"U+{code:04X}" for code in sorted(regressions)),
            file=sys.stderr,
        )
        return 1 if args.check else 0
    accepted = f" ({len(KNOWN_DEFAULT_GAPS)} recorded gaps aside)" if KNOWN_DEFAULT_GAPS else ""
    print(f"\n{DEFAULT_FACE}: covers everything techxt emits{accepted}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
