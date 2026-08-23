//! TEMPORARY measurement harness — deleted before the phase ends.

use std::time::Instant;

use techxt::Converter;

#[test]
fn probe() {
    let doc = std::env::var("PROBE_DOC").unwrap_or_else(|_| r"\def\x{\x}\x".into());
    let depth: usize = std::env::var("PROBE_DEPTH")
        .unwrap_or_else(|_| "256".into())
        .parse()
        .unwrap();
    let count: usize = std::env::var("PROBE_COUNT")
        .unwrap_or_else(|_| "100000".into())
        .parse()
        .unwrap();
    let converter = Converter::builder()
        .expansion_depth_limit(depth)
        .expansion_count_limit(count)
        .build()
        .expect("builds");
    let start = Instant::now();
    let conversion = converter.latex_to_text(&doc).expect("parses");
    let parsed = start.elapsed();
    let ids: Vec<&str> = conversion
        .diagnostics
        .iter()
        .map(|d| d.identifier())
        .collect();
    let mut counted: Vec<(&str, usize)> = Vec::new();
    for id in &ids {
        match counted.iter_mut().find(|(name, _)| name == id) {
            Some((_, n)) => *n += 1,
            None => counted.push((id, 1)),
        }
    }
    println!(
        "RESULT doc={doc:?} depth={depth} count={count} elapsed={:?} text_len={} diags={} {:?} errors={}",
        parsed,
        conversion.text.len(),
        ids.len(),
        counted,
        conversion.diagnostics.has_errors(),
    );
    let dropping = Instant::now();
    drop(conversion);
    println!("DROP {:?}", dropping.elapsed());
}
