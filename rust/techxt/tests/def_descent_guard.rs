//! The descent guard relay (DECISIONS.md D9).
//!
//! techy has one descent guard and two places that need it: the parser, which spends
//! stack per syntactic nesting level, and the recomposer's fold, which spends it per
//! tree nesting level. `ConverterBuilder::descent_guard` configures both from one call,
//! and techxt invents no limit of its own — the library default is techy's.

use techxt::convert::{StdDescentGuard, StdDescentGuardInit};
use techxt::Converter;

/// `depth` nested groups around `text`.
fn nested(text: &str, depth: usize) -> String {
    format!("{}{text}{}", "{".repeat(depth), "}".repeat(depth))
}

/// A converter with an explicit descent guard.
fn with_guard(init: StdDescentGuardInit) -> Converter {
    Converter::builder()
        .descent_guard(init)
        .build()
        .expect("the placeholder definitions build")
}

#[test]
fn the_guard_reaches_the_parser() {
    // A parse costs about two descents per syntactic nesting level, so a limit of 24
    // admits some ten levels and refuses hundreds — deterministically, in any build
    // profile, which a stack budget cannot promise.
    let converter = with_guard(StdDescentGuardInit::depth_limit(24));
    assert_eq!(
        converter
            .latex_to_text(&nested("x", 6))
            .expect("inside the limit")
            .text,
        "x\n"
    );
    let error = converter
        .latex_to_text(&nested("x", 400))
        .expect_err("past the limit");
    assert_eq!(error.identifier(), "core.constructs.descent-limit-exceeded");
}

#[test]
fn the_guard_reaches_the_fold() {
    // The fold's limit is only reachable on a tree techxt did not parse itself, since
    // one parse costs more descents than the fold of what it produced. Parsing with a
    // generous guard and folding with a tight one is exactly the shape of a caller
    // handing `tree_to_text` a tree from elsewhere.
    let parser = with_guard(StdDescentGuardInit::depth_limit(200));
    let tree = parser
        .language()
        .parse(nested("x", 30))
        .expect("parses")
        .tree;

    let renderer = with_guard(StdDescentGuardInit::depth_limit(4));
    let conversion = renderer.tree_to_text(&tree);
    // PLAN.md §10.4: the one fold abort gives empty output and an error diagnostic.
    assert_eq!(conversion.text, "");
    assert_eq!(
        conversion
            .diagnostics
            .with_identifier("techxt.render-aborted")
            .count(),
        1
    );
    assert!(conversion.diagnostics.has_errors());

    // The same tree through a converter whose guard admits it converts normally.
    assert_eq!(parser.tree_to_text(&tree).text, "x\n");
}

#[test]
fn the_guard_can_be_turned_off() {
    let converter = with_guard(StdDescentGuardInit::off());
    assert_eq!(
        converter
            .latex_to_text(&nested("x", 30))
            .expect("no limit")
            .text,
        "x\n"
    );
}

#[test]
fn the_default_guard_is_techys_own() {
    // Not a limit techxt invented: a library cannot measure the caller's stack, and a
    // fixed budget larger than techy's is unsafe on a small-stack thread. What the
    // default guarantees is that a pathological document is refused rather than
    // overflowing the stack.
    assert_eq!(StdDescentGuard::DEFAULT_STACK_BUDGET, 250 * 1024);
    let error = Converter::standard()
        .latex_to_text(&"{".repeat(2000))
        .expect_err("the default guard refuses");
    assert_eq!(error.identifier(), "core.constructs.descent-limit-exceeded");
}

#[test]
fn the_default_guard_warns_before_it_refuses() {
    // techy's unconfigured default emits a one-time warning at half the budget, so a
    // document approaching the limit says so while its conversion still succeeds. How
    // deep that is depends on the build profile, which is why the limit is searched for
    // rather than assumed.
    let converter = Converter::standard();
    let mut warned = false;
    for depth in 1..200 {
        let Ok(conversion) = converter.latex_to_text(&nested("x", depth)) else {
            // The guard refused: everything shallower has been seen.
            break;
        };
        warned |= conversion
            .diagnostics
            .with_identifier("core.constructs.descent-limit-approaching")
            .count()
            > 0;
    }
    assert!(warned, "the default guard should warn before refusing");
}

#[test]
fn a_fold_that_only_approaches_the_limit_warns_rather_than_aborting() {
    // DECISIONS.md D12. The fold-side early warning used to be minted as
    // `techxt.render-aborted` at error severity, so a conversion that produced complete
    // text still reported a failure — and PLAN.md §13 turns a failure into exit code 1.
    // The warning must read like the parse side's: same condition, warning severity, and
    // no error anywhere.
    let parser = with_guard(StdDescentGuardInit::depth_limit(2000));
    // The default guard is the one that warns at half its budget; how deep that is
    // depends on the build profile, so the depth is searched for rather than assumed.
    let renderer = Converter::standard();

    let mut warned = false;
    for depth in 1..400 {
        let tree = parser
            .language()
            .parse(nested("x", depth))
            .expect("parses")
            .tree;
        let conversion = renderer.tree_to_text(&tree);
        if conversion.text.is_empty() {
            // The guard refused this one: that is the genuine abort, and everything
            // shallower has been seen.
            break;
        }
        if conversion
            .diagnostics
            .with_identifier("core.constructs.descent-limit-approaching")
            .count()
            > 0
        {
            warned = true;
            assert_eq!(conversion.text, "x\n");
            assert!(
                !conversion.diagnostics.has_errors(),
                "a completed conversion reported an error: {:?}",
                conversion.diagnostics
            );
            assert_eq!(
                conversion
                    .diagnostics
                    .with_identifier("techxt.render-aborted")
                    .count(),
                0
            );
            break;
        }
    }
    assert!(warned, "the default guard should warn before refusing");
}
