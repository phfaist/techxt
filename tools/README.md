# tools/ — dev-only scripts

Scripts that generate checked-in Rust source for the `techxt` library. They are
development aids: they are not part of the cargo build, are never run at compile
time, and nothing in `rust/` depends on them at runtime.

Planned content (milestone M4, PLAN.md §12.4): generation of the accent-combining
and math-alphabet tables in `techxt::defs` and `techxt::mathfmt` from upstream
unicode data, so those tables can be regenerated and reviewed as a diff instead of
being hand-edited.

Regenerated output is committed alongside the script; re-running a script must
produce byte-identical files unless the upstream data changed.
