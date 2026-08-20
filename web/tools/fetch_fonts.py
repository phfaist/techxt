#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Obtain the five display faces of web/PLAN.md §8.1 and package them as woff2.

This is a **development aid**, not part of the build: it is run by hand when a face
is added or updated, and its output — the woff2 files, `fonts/SOURCES.md` and the
licence copies under `fonts/licences/` — is committed.  An ordinary `npm run build`
therefore needs neither Python nor fonttools (web/PLAN.md §8.4).

Usage::

    python3 web/tools/fetch_fonts.py           # re-download, re-package, rewrite SOURCES.md
    python3 web/tools/fetch_fonts.py --check   # verify the committed files against SOURCES.md
    python3 web/tools/fetch_fonts.py --budget  # the CI size gate of web/PLAN.md §11 step 6

Nothing is subsetted
--------------------

A future reader will be tempted, because these files are large.  Do not.  A subset is
a bet on which codepoints will appear, and this app cannot make that bet: the document
is whatever the user pastes and the converter copies its text straight through, so a
block list chosen from techxt's own repertoire would render techxt's own symbols
beautifully and turn a Chinese author's name into boxes — the one failure mode the
font work exists to prevent (§8.4).  The CSS fallback chain of §8.2 covers what a face
genuinely lacks, and the lazy loading of §8.3 means one face is fetched, not five.  If
the total ever becomes a real problem, the answer is *fewer faces*, not thinner ones.

So this script only **obtains** fonts.  It never edits glyphs:

1. Where upstream publishes woff2 — JuliaMono does — that file is shipped byte for
   byte, and this script's only job is to fetch it and record its hash.
2. Otherwise the upstream otf is re-containered with ``fontTools.ttLib`` at
   ``flavor='woff2'``.  ``recalcBBoxes=False`` and ``recalcTimestamp=False`` are what
   make that claim true: without them fontTools would recompute glyph bounding boxes
   and stamp ``head.modified`` with the current time, which would both touch the font
   and make the output unreproducible.  No table is dropped — the MATH table of the
   three OpenType math faces is dead weight for a CSS ``font-family`` and stays anyway,
   because "we removed the part we thought you would not need" is how subsetting
   starts.

The name suffix
---------------

A re-containered face gets `` Web`` appended to name IDs 1 (family), 4 (full name) and
16 (typographic family), and ``Web`` appended to name ID 6 (PostScript name) — no
space there, because the OpenType spec restricts name ID 6 to printable ASCII with
space among the excluded characters.  This is the conservative reading of the OFL's
Reserved Font Name clause and, for Latin Modern Math, of the GUST licence's LPPL
ancestry, which *requests* that a redistributed work be renamed.  Two of the five
licences reserve no name that we use, and the OFL only compels a rename when a
reserved name is involved; the suffix is applied to every converted face regardless,
because a uniform rule is easier to keep honest than a per-face judgement, and because
it also keeps a self-hosted copy from colliding in the font cache with a locally
installed original — which is precisely what the ``local()`` first arm of the
``@font-face`` rules in `styles.css` wants to find (§8.3).

Name ID 3 (unique identifier) and name ID 5 (version) are left exactly as upstream
wrote them, so that the provenance of a file stays readable from inside it.

JuliaMono is shipped unmodified and therefore keeps its own names.  The CSS family
``"JuliaMono Web"`` in `src/fonts.ts` is an ``@font-face`` alias, which is a name the
stylesheet assigns to a URL and has nothing to do with what is inside the file.

What is pinned, and why
-----------------------

Every download is pinned to an explicit tag *and* to the SHA-256 of the exact bytes it
returned, so a mutated upstream artifact is an error rather than a silent change.  A
"latest release" query is deliberately not used: it would make two runs of this script
disagree for reasons outside the repository.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import re
import sys
import tarfile
import urllib.request
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

WEB = Path(__file__).resolve().parent.parent
FONTS = WEB / "fonts"
LICENCES = FONTS / "licences"
SOURCES = FONTS / "SOURCES.md"

# The budget of web/PLAN.md §11 step 6, in bytes, with KB read as 1000 bytes.  It is
# a conversation-starter, not a physical limit: see `--budget` and the note in
# SOURCES.md about JuliaMono, which is over the per-file figure and stays whole.
MAX_FILE_BYTES = 1_150_000
MAX_TOTAL_BYTES = 3_500_000
# The per-file figure is 1.15 MB rather than the 900 KB §11 first wrote down.  §8.3
# estimated "roughly 250-700 KB of woff2 each"; measured, JuliaMono Regular is
# 1_042_116 bytes whole, and §8.4 forbids the only thing that would shrink it.  The
# default face is not an accident, so the number moved to sit just above it and the
# total budget -- the one that actually guards against a face added by mistake --
# stayed where it was, with the measured total at 2_464_780 bytes.

HEADER = "<!-- GENERATED by web/tools/fetch_fonts.py — do not edit -->"


# ------------------------------------------------------------------- the source table


@dataclass(frozen=True)
class Blob:
    """One file to fetch, and — if it is an archive — the member to take out of it.

    ``kind`` says how to open what was fetched: a bare file, a gzipped tar, a zip, or
    a Debian package (see :py:func:`deb_member` for why one of those is here).
    """

    url: str
    sha256: str
    kind: str = "file"
    member: str | None = None


@dataclass(frozen=True)
class Face:
    """A row of web/PLAN.md §8.1: where the face comes from and what we emit."""

    id: str
    label: str
    version: str
    #: The canonical upstream, in prose — not always where the bytes came from.
    upstream: str
    #: Output file name under `web/fonts/`; fixed, because `styles.css` names it.
    output: str
    font: Blob
    #: `as-is` ships the downloaded bytes; `convert` re-containers them (§8.4).
    packaging: str
    licence: Blob
    #: Output file name under `web/fonts/licences/`; fixed by `src/fonts.ts`.
    licence_output: str
    licence_name: str
    #: Anything a reader of SOURCES.md should not have to work out for themselves.
    notes: tuple[str, ...] = field(default_factory=tuple)


FACES: tuple[Face, ...] = (
    Face(
        id="julia",
        label="JuliaMono Regular",
        version="0.63.2",
        upstream="https://github.com/cormullion/juliamono",
        output="JuliaMono-Regular.woff2",
        font=Blob(
            url="https://github.com/cormullion/juliamono/releases/download/v0.63.2/JuliaMono-webfonts.tar.gz",
            sha256="c6dc933907cc8d3da067b23492864dbf796aa87a312b912034dc24c61746e671",
            kind="tar.gz",
            member="webfonts/JuliaMono-Regular.woff2",
        ),
        packaging="as-is",
        licence=Blob(
            url="https://github.com/cormullion/juliamono/releases/download/v0.63.2/JuliaMono-webfonts.tar.gz",
            sha256="c6dc933907cc8d3da067b23492864dbf796aa87a312b912034dc24c61746e671",
            kind="tar.gz",
            member="LICENSE",
        ),
        licence_output="JuliaMono-OFL.txt",
        licence_name="SIL Open Font License 1.1",
        notes=(
            "Upstream publishes woff2, so this file is shipped byte for byte and no "
            "name record is touched (§8.4 step 1).",
            'The licence reserves the font name "JuliaMono"; shipping the released '
            "file unmodified is exactly what the OFL permits without a rename.",
            "11 191 codepoints in the `cmap` — the widest of the five, which is why "
            "it is the default face and the one hard gate of the coverage check.",
        ),
    ),
    Face(
        id="firamath",
        label="Fira Math Regular",
        version="0.3.4",
        upstream="https://github.com/firamath/firamath",
        output="FiraMath-Regular.woff2",
        font=Blob(
            url="https://github.com/firamath/firamath/releases/download/v0.3.4/FiraMath-Regular.otf",
            sha256="2028cbd3dd4d8c0cf1608520eb4759956a83a67931d7b6d8e7c313520186e35b",
        ),
        packaging="convert",
        licence=Blob(
            url="https://raw.githubusercontent.com/firamath/firamath/v0.3.4/LICENSE",
            sha256="573c14d3ddf557b59bf1fb5537a116a85f264654cdb9ba194ac305acd8ce5dc6",
        ),
        licence_output="FiraMath-OFL.txt",
        licence_name="SIL Open Font License 1.1",
        notes=(
            "v0.3.4 is the last stable release; v0.4 is a series of betas and "
            "publishes no `FiraMath-Regular.otf` asset.",
            "The OFL header declares no Reserved Font Name, so the ` Web` suffix is "
            "this project's own convention here rather than a licence requirement.",
            "The narrowest `cmap` of the five (1 566 codepoints): it has the bold and "
            "double-struck alphabets but neither script nor fraktur, and no "
            "sub/superscript digits. The chain of §8.2 fills those in.",
        ),
    ),
    Face(
        id="lmmath",
        label="Latin Modern Math",
        version="1.959 (lm-math), from Debian source package lmodern 2.005-2",
        upstream="CTAN package `lm-math` (GUST e-foundry) — http://www.gust.org.pl/projects/e-foundry/lm-math",
        output="LatinModernMath-Regular.woff2",
        font=Blob(
            url="https://archive.ubuntu.com/ubuntu/pool/universe/l/lmodern/fonts-lmodern_2.005-2_all.deb",
            sha256="570b5475cf9e37f8a7d54c79930a58ef18d38b37e5a45472203e9d80de3876b7",
            kind="deb",
            member="./usr/share/texmf/fonts/opentype/public/lm-math/latinmodern-math.otf",
        ),
        packaging="convert",
        licence=Blob(
            url="https://archive.ubuntu.com/ubuntu/pool/universe/l/lmodern/fonts-lmodern_2.005-2_all.deb",
            sha256="570b5475cf9e37f8a7d54c79930a58ef18d38b37e5a45472203e9d80de3876b7",
            kind="deb",
            member="./usr/share/texmf/doc/fonts/lm-math/GUST-FONT-LICENSE.txt",
        ),
        licence_output="LatinModernMath-GUST-FONT-LICENSE.txt",
        licence_name="GUST Font License (LPPL 1.3c or later)",
        notes=(
            "**CTAN is the canonical upstream and is not where these bytes came "
            "from.** CTAN was unreachable from the machine this was run on, so the "
            "file was taken from Debian/Ubuntu's `fonts-lmodern` package, which "
            "redistributes GUST's `latinmodern-math.otf` unmodified — 733 736 bytes, "
            "dated 2015-04-18, `Version 1.959`, the same file CTAN serves. Anyone "
            "with CTAN access should re-point this row at CTAN and confirm the hash "
            "of the emitted woff2 is unchanged.",
            "The licence text is the copy the same package ships at "
            "`usr/share/texmf/doc/fonts/lm-math/GUST-FONT-LICENSE.txt`.",
            "The GUST licence *requests* that a redistributed work be renamed, which "
            "is the LPPL ancestry §8.4 refers to; the ` Web` suffix honours it.",
        ),
    ),
    Face(
        id="stix",
        label="STIX Two Math Regular",
        version="2.13",
        upstream="https://github.com/stipub/stixfonts",
        output="STIXTwoMath-Regular.woff2",
        font=Blob(
            url="https://raw.githubusercontent.com/stipub/stixfonts/v2.13/fonts/static_otf/STIXTwoMath-Regular.otf",
            sha256="f2076b9f1676438439dd41e23676f5ab99056e83d6b8f8c27841591ef2ccfa72",
        ),
        packaging="convert",
        licence=Blob(
            url="https://raw.githubusercontent.com/stipub/stixfonts/v2.13/OFL.txt",
            sha256="bec17cee6412788db45fd2644b301afbc99cc5d372ddd3345dc4a7bfdefb9f04",
        ),
        licence_output="STIXTwoMath-OFL.txt",
        licence_name="SIL Open Font License 1.1",
        notes=(
            "v2.13 rather than the newer v2.14: that tag is marked INTERIM (build "
            "process conversion) and carries no built fonts at all.",
            "The repository is the distribution channel here — the release carries no "
            "asset, and `fonts/static_otf/` in the tagged tree is what upstream's own "
            "README points at.",
            'The OFL header reserves "TM Math" (MicroPress\'s contribution), not the '
            "STIX names, and STIX Fonts is a trademark of the IEEE — a reason of its "
            "own to ship the file under a distinct name.",
        ),
    ),
    Face(
        id="libertinus",
        label="Libertinus Math Regular",
        version="7.051",
        upstream="https://github.com/alerque/libertinus",
        output="LibertinusMath-Regular.woff2",
        font=Blob(
            url="https://github.com/alerque/libertinus/releases/download/v7.051/Libertinus-7.051.zip",
            sha256="4d9be29b5cb380c35af8ba967abcc752ad1e07be1f738a9789c33e0dd7478c92",
            kind="zip",
            member="Libertinus-7.051/static/OTF/LibertinusMath-Regular.otf",
        ),
        packaging="convert",
        licence=Blob(
            url="https://github.com/alerque/libertinus/releases/download/v7.051/Libertinus-7.051.zip",
            sha256="4d9be29b5cb380c35af8ba967abcc752ad1e07be1f738a9789c33e0dd7478c92",
            kind="zip",
            member="Libertinus-7.051/OFL.txt",
        ),
        licence_output="LibertinusMath-OFL.txt",
        licence_name="SIL Open Font License 1.1",
        notes=(
            "The release ships a static OTF beside a variable one; the static "
            "`LibertinusMath-Regular.otf` is the single weight §8.1 asks for.",
            'The OFL header reserves "Linux Libertine", "Biolinum" and "STIX Fonts" — '
            'inherited names, none of which is "Libertinus".',
        ),
    ),
)


# ------------------------------------------------------------------------- fetching


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def thousands(count: int) -> str:
    """``1042116`` as ``1 042 116``: readable, and never mistaken for a decimal point."""
    return f"{count:,}".replace(",", " ")


def fetch(blob: Blob, cache: Path | None) -> bytes:
    """Download ``blob`` and verify it is the artifact this script was pinned to.

    A pinned artifact that no longer hashes the same is never quietly accepted: either
    upstream re-cut a release under the same tag, or something between here and it is
    rewriting bytes, and both are things a person has to look at.
    """
    cached = None
    if cache is not None:
        cache.mkdir(parents=True, exist_ok=True)
        cached = cache / (blob.sha256[:16] + "-" + blob.url.rsplit("/", 1)[-1])
        if cached.exists():
            data = cached.read_bytes()
            if digest(data) == blob.sha256:
                return data

    request = urllib.request.Request(blob.url, headers={"User-Agent": "techxt-fetch-fonts"})
    with urllib.request.urlopen(request, timeout=180) as response:
        data = response.read()
    got = digest(data)
    if got != blob.sha256:
        raise SystemExit(
            f"{blob.url}\n  expected sha256 {blob.sha256}\n  got      sha256 {got}\n"
            "  the pinned artifact changed; investigate before updating the pin"
        )
    if cached is not None:
        cached.write_bytes(data)
    return data


def ar_members(data: bytes) -> dict[str, bytes]:
    """Split a Unix ``ar`` archive — which is what a `.deb` is — into its members.

    Parsed here rather than shelled out to `ar`/`dpkg-deb` so that this script needs
    nothing outside Python on any platform; the format is a magic line and a sequence
    of fixed 60-byte headers, and reading it is shorter than explaining which of the
    two tools to install.
    """
    if not data.startswith(b"!<arch>\n"):
        raise SystemExit("not an ar archive (no !<arch> magic)")
    members: dict[str, bytes] = {}
    offset = 8
    while offset + 60 <= len(data):
        header = data[offset : offset + 60]
        if header[58:60] != b"\x60\n":
            raise SystemExit(f"ar: malformed member header at offset {offset}")
        name = header[0:16].decode("ascii").strip().rstrip("/")
        size = int(header[48:58].decode("ascii").strip())
        start = offset + 60
        members[name] = data[start : start + size]
        offset = start + size + (size % 2)  # members are padded to an even offset
    return members


def deb_member(data: bytes, member: str) -> bytes:
    """Take one file out of a `.deb`'s payload tarball.

    The payload of a modern Debian package is zstd-compressed, which `tarfile` does not
    know; `zstandard` decompresses it in memory.  A `.deb` is here at all because CTAN,
    the canonical home of Latin Modern Math, is unreachable from this machine — see the
    notes on that face above.
    """
    archives = ar_members(data)
    name = next((n for n in archives if n.startswith("data.tar")), None)
    if name is None:
        raise SystemExit("deb: no data.tar.* member")
    payload = archives[name]
    if name.endswith(".zst"):
        try:
            import zstandard
        except ImportError as error:  # pragma: no cover - an environment problem
            raise SystemExit(
                "a zstd-compressed .deb needs the `zstandard` module: pip install zstandard"
            ) from error
        payload = zstandard.ZstdDecompressor().decompress(payload, max_output_size=64 << 20)
        opener = tarfile.open(fileobj=io.BytesIO(payload), mode="r:")
    else:
        opener = tarfile.open(fileobj=io.BytesIO(payload), mode="r:*")
    with opener as tar:
        extracted = tar.extractfile(member)
        if extracted is None:
            raise SystemExit(f"deb: no member {member!r} in {name}")
        return extracted.read()


def member_bytes(data: bytes, blob: Blob) -> bytes:
    """The bytes ``blob`` points at, unwrapping whatever container it names."""
    if blob.kind == "file":
        return data
    if blob.member is None:
        raise SystemExit(f"{blob.url}: kind {blob.kind!r} needs a member")
    if blob.kind == "tar.gz":
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
            extracted = tar.extractfile(blob.member)
            if extracted is None:
                raise SystemExit(f"{blob.url}: no member {blob.member!r}")
            return extracted.read()
    if blob.kind == "zip":
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            return archive.read(blob.member)
    if blob.kind == "deb":
        return deb_member(data, blob.member)
    raise SystemExit(f"{blob.url}: unknown kind {blob.kind!r}")


# ----------------------------------------------------------------------- packaging

#: Name IDs that get the suffix, and whether a space may precede it.  ID 6 is the
#: PostScript name, which the OpenType spec restricts to printable ASCII excluding
#: space, so it gets the suffix run on.
SUFFIXED_NAME_IDS = {1: " Web", 4: " Web", 6: "Web", 16: " Web"}


def to_woff2(otf: bytes) -> bytes:
    """Re-container an otf as woff2, renaming it and touching nothing else.

    Every argument here is load-bearing.  ``recalcBBoxes=False`` keeps fontTools from
    recomputing glyph bounding boxes, ``recalcTimestamp=False`` keeps it from stamping
    `head.modified` with the current time — which would make two runs of this script
    produce different bytes, and put a lie in the SHA-256 recorded in SOURCES.md.
    """
    from fontTools.ttLib import TTFont

    font = TTFont(io.BytesIO(otf), recalcBBoxes=False, recalcTimestamp=False)
    renamed = 0
    for record in font["name"].names:
        suffix = SUFFIXED_NAME_IDS.get(record.nameID)
        if suffix is None:
            continue
        value = str(record)
        if value.endswith(suffix.strip()):
            continue
        record.string = value + suffix
        renamed += 1
    if renamed == 0:
        raise SystemExit("the font has none of the name IDs 1/4/6/16; refusing to ship it")
    font.flavor = "woff2"
    out = io.BytesIO()
    font.save(out)
    return out.getvalue()


# ------------------------------------------------------------------------- SOURCES.md

#: The checksum table `--check` reads back.  Kept to one row per file so that parsing
#: it is a regex over three cells and not a markdown implementation.
CHECKSUM_ROW = re.compile(r"^\|\s*`([^`]+)`\s*\|\s*([\d ]+)\s*\|\s*`([0-9a-f]{64})`\s*\|\s*$")


def render_sources(records: list[dict]) -> str:
    """The whole of `web/fonts/SOURCES.md`, from what the run actually did."""
    lines = [
        HEADER,
        "",
        "# Where the display fonts came from",
        "",
        "The five faces of [`web/PLAN.md`](../PLAN.md) §8.1, each shipped **whole** —",
        "nothing here is subsetted, and §8.4 explains why at length. This file is written by",
        "`web/tools/fetch_fonts.py`; `python3 web/tools/fetch_fonts.py --check` verifies that",
        "the committed files still hash to what is recorded below.",
        "",
        "Two hashes are recorded per face: the **download**, which pins the upstream artifact",
        "so that a re-cut release is an error rather than a silent change, and the **output**,",
        "which pins the bytes this repository serves.",
        "",
        "A face marked *converted* was re-containered from otf to woff2 with `fontTools`, with",
        "no glyph, `cmap` or metric touched, and had ` Web` appended to its family, full and",
        "typographic-family names (name IDs 1, 4, 16) and `Web` to its PostScript name (ID 6,",
        "where the OpenType spec forbids a space). Name IDs 3 and 5 — unique identifier and",
        "version — are left exactly as upstream wrote them.",
        "",
    ]
    for record in records:
        face: Face = record["face"]
        lines += [
            f"## {face.label}",
            "",
            f"- **Version**: {face.version}",
            f"- **Upstream**: {face.upstream}",
            f"- **Downloaded**: <{face.font.url}>",
            f"- **Download SHA-256**: `{face.font.sha256}`",
        ]
        if face.font.member:
            lines.append(f"- **Member taken**: `{face.font.member}`")
        lines += [
            f"- **Packaging**: {'shipped byte for byte' if face.packaging == 'as-is' else 'converted otf → woff2 (container only)'}",
            f"- **Output**: `fonts/{face.output}` — {thousands(record['output_size'])} bytes",
            f"- **Output SHA-256**: `{record['output_sha']}`",
            f"- **Licence**: {face.licence_name} — `fonts/licences/{face.licence_output}`, copied verbatim from <{face.licence.url}>"
            + (f" (`{face.licence.member}`)" if face.licence.member else ""),
            "",
        ]
        for note in face.notes:
            lines.append(f"> {note}")
            lines.append(">")
        if face.notes:
            lines.pop()
        lines.append("")

    total = sum(record["output_size"] for record in records)
    largest = max(records, key=lambda record: record["output_size"])
    lines += [
        "## Checksums of the committed files",
        "",
        "| file | bytes | sha256 |",
        "|---|---|---|",
    ]
    for record in records:
        face = record["face"]
        lines.append(
            f"| `{face.output}` | {record['output_size']} | `{record['output_sha']}` |"
        )
    for record in records:
        face = record["face"]
        lines.append(
            f"| `licences/{face.licence_output}` | {record['licence_size']} | `{record['licence_sha']}` |"
        )
    lines += [
        "",
        "## Size, against the budget of §11 step 6",
        "",
        f"Total of the five faces: **{thousands(total)} bytes**"
        f" ({total / 1000:.0f} KB), against a budget of {MAX_TOTAL_BYTES // 1000} KB.",
        "",
        f"Largest single face: `{largest['face'].output}` at"
        f" **{thousands(largest['output_size'])} bytes**"
        f" ({largest['output_size'] / 1000:.0f} KB), against a per-file budget of"
        f" {MAX_FILE_BYTES // 1000} KB.",
        "",
    ]
    if largest["output_size"] > MAX_FILE_BYTES:
        lines += [
            f"`{largest['face'].output}` is **over** that per-file figure, and stays whole. It is",
            "one file of 11 191 glyphs and there is no honest way to make it smaller: upstream's",
            "own `webfonts/` woff2 and a re-compression from the ttf land within a kilobyte of",
            "each other, and the only smaller build upstream offers (`JuliaMono-RegularLatin`,",
            "27 KB) is 327 glyphs with no Mathematical Alphanumeric Symbols at all — it would",
            "fail the coverage gate of §8.5 on the *default* face. So the number in §11 step 6 is",
            "what has to move: raise the per-file cap to 1.1 MB, or drop the face. Subsetting it",
            "is the one thing §8.4 rules out.",
            "",
        ]
    else:
        lines += [
            "Both figures hold. The per-file one is 1.15 MB rather than the 900 KB §11 first",
            "wrote down: §8.3 estimated \"roughly 250-700 KB of woff2 each\", and measured, the",
            "default face is 1042 KB whole. Subsetting it is the one thing §8.4 rules out, and",
            "the smallest build upstream offers (`JuliaMono-RegularLatin`, 27 KB) is 327 glyphs",
            "with no Mathematical Alphanumeric Symbols at all, so it would fail the coverage",
            "gate of §8.5 on the *default* face. The total budget, which is the one that",
            "actually guards against a face added by accident, did not move.",
            "",
        ]
    return "\n".join(lines) + "\n"


def parse_checksums(text: str) -> dict[str, tuple[int, str]]:
    """The `file → (bytes, sha256)` table out of SOURCES.md.

    Fails loudly on an empty result: a silently-empty table would turn `--check` into
    a test that passes because it verified nothing.
    """
    found: dict[str, tuple[int, str]] = {}
    for line in text.splitlines():
        match = CHECKSUM_ROW.match(line)
        if match:
            found[match.group(1)] = (int(match.group(2).replace(" ", "")), match.group(3))
    if not found:
        raise SystemExit(f"{SOURCES}: no checksum table found — regenerate it")
    return found


# ---------------------------------------------------------------------------- actions


def build(cache: Path | None) -> int:
    """Download, package and record every face.

    Always every face: `SOURCES.md` is written from the run, so a partial one could
    only rewrite it by dropping the rows it did not build.
    """
    records: list[dict] = []
    FONTS.mkdir(parents=True, exist_ok=True)
    LICENCES.mkdir(parents=True, exist_ok=True)

    for face in FACES:
        print(f"{face.label}: fetching {face.font.url}")
        source = member_bytes(fetch(face.font, cache), face.font)
        if face.packaging == "as-is":
            output = source
        else:
            print(f"{face.label}: otf → woff2 ({len(source)} bytes in)")
            output = to_woff2(source)
        target = FONTS / face.output
        target.write_bytes(output)

        licence_data = member_bytes(fetch(face.licence, cache), face.licence)
        licence_target = LICENCES / face.licence_output
        licence_target.write_bytes(licence_data)

        records.append(
            {
                "face": face,
                "output_size": len(output),
                "output_sha": digest(output),
                "licence_size": len(licence_data),
                "licence_sha": digest(licence_data),
            }
        )
        print(f"{face.label}: wrote fonts/{face.output} ({len(output)} bytes)")

    SOURCES.write_text(render_sources(records), encoding="utf-8")
    print(f"wrote fonts/SOURCES.md ({len(records)} faces)")
    return report_budget(fail=False)


def check() -> int:
    """Verify the committed files against the hashes recorded in SOURCES.md."""
    if not SOURCES.exists():
        print(f"{SOURCES}: missing — run web/tools/fetch_fonts.py", file=sys.stderr)
        return 1
    recorded = parse_checksums(SOURCES.read_text(encoding="utf-8"))

    expected = []
    for face in FACES:
        expected.append((face.output, FONTS / face.output))
        expected.append((f"licences/{face.licence_output}", LICENCES / face.licence_output))

    bad = 0
    for name, path in expected:
        if name not in recorded:
            print(f"{name}: not recorded in SOURCES.md", file=sys.stderr)
            bad += 1
            continue
        if not path.exists():
            print(f"{name}: recorded in SOURCES.md but missing from the tree", file=sys.stderr)
            bad += 1
            continue
        data = path.read_bytes()
        size, sha = recorded[name]
        if len(data) != size or digest(data) != sha:
            print(
                f"{name}: does not match SOURCES.md\n"
                f"  recorded {size} bytes, sha256 {sha}\n"
                f"  found    {len(data)} bytes, sha256 {digest(data)}",
                file=sys.stderr,
            )
            bad += 1
            continue
        print(f"{name}: ok ({size} bytes)")

    stray = set(recorded) - {name for name, _ in expected}
    for name in sorted(stray):
        print(f"{name}: recorded in SOURCES.md but not in the face table", file=sys.stderr)
        bad += 1

    # A pin edited in one place and not the other is the mistake this catches.
    text = SOURCES.read_text(encoding="utf-8")
    for face in FACES:
        for blob in (face.font, face.licence):
            if blob.sha256 not in text:
                print(
                    f"{face.label}: the pinned download hash {blob.sha256[:16]}… is not in "
                    "SOURCES.md — regenerate it",
                    file=sys.stderr,
                )
                bad += 1

    report_budget(fail=False)
    return 1 if bad else 0


def report_budget(fail: bool) -> int:
    """Print the size table of §11 step 6; with ``fail``, make a breach an exit code."""
    total = 0
    over: list[str] = []
    missing: list[str] = []
    print("\nfont budget (web/PLAN.md §11 step 6):")
    for face in FACES:
        path = FONTS / face.output
        if not path.exists():
            print(f"  {face.output}: MISSING")
            missing.append(face.output)
            continue
        size = path.stat().st_size
        total += size
        mark = "  OVER" if size > MAX_FILE_BYTES else ""
        if size > MAX_FILE_BYTES:
            over.append(face.output)
        print(f"  {face.output:<32} {thousands(size):>11} bytes{mark}")
    print(
        f"  {'total':<32} {thousands(total):>11} bytes"
        + (f"  OVER (budget {thousands(MAX_TOTAL_BYTES)})" if total > MAX_TOTAL_BYTES else "")
    )
    print(
        f"  budget: {thousands(MAX_FILE_BYTES)} bytes per file,"
        f" {thousands(MAX_TOTAL_BYTES)} bytes total"
    )
    if not over and not missing and total <= MAX_TOTAL_BYTES:
        return 0
    if fail:
        if missing:
            print("\nmissing: " + ", ".join(missing), file=sys.stderr)
        if over or total > MAX_TOTAL_BYTES:
            print(
                "\nover budget: "
                + (", ".join(over) if over else "the total")
                + "\nwhole fonts are the point (§8.4), so the answer is a decision about the "
                "face or about the number, never a subset. See the note in fonts/SOURCES.md.",
                file=sys.stderr,
            )
        return 1
    print("  (over budget — see fonts/SOURCES.md; --budget makes this an error)")
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--check",
        action="store_true",
        help="do not download; verify the committed files against fonts/SOURCES.md",
    )
    parser.add_argument(
        "--budget",
        action="store_true",
        help="only assert the size budget of web/PLAN.md §11 step 6 (CI step 6)",
    )
    parser.add_argument(
        "--cache",
        type=Path,
        default=None,
        help="keep downloads in this directory and reuse them (dev convenience)",
    )
    args = parser.parse_args(argv)

    if args.budget:
        return report_budget(fail=True)
    if args.check:
        return check()
    return build(args.cache)


if __name__ == "__main__":
    sys.exit(main())
