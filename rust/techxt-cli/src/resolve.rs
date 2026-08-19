//! The sandboxed filesystem resolver behind `--input-dir` (PLAN.md §13).
//!
//! techy resolves `\input` at *parse* time, and it deliberately assigns no meaning at
//! all to the reference string: path syntax, extension conventions, sandboxing and
//! recursion policy are the embedder's, which for this program means this module.
//!
//! [`DirectoryResolver`] answers a reference only when the file it names lies inside the
//! directory the user named, and it decides that on the **canonical** path — the one
//! `realpath(3)` reports, with `..` segments collapsed and every symbolic link followed.
//! Anything else is a sandbox with a hole in it: `\input{../../etc/passwd}` and a
//! symlink pointing out of the tree both look perfectly ordinary before canonicalization.

use std::path::{Path, PathBuf, MAIN_SEPARATOR_STR};
use std::sync::Arc;

use techy::source::{
    check_include_chain, ResolveError, ResolvedContent, SourceResolver, SourceSpan,
};

/// How deep `\input` may nest before the resolver calls it a runaway.
///
/// The chain check reports a cycle and a depth overflow with distinct messages, so this
/// is a backstop for the acyclic-but-absurd case (a generated tree of includes), not the
/// recursion guard itself.
const MAX_INCLUDE_DEPTH: usize = 20;

/// The extensions tried, in order, when the reference names no readable file itself.
///
/// LaTeX writes `\input{chapters/intro}` and means `chapters/intro.tex`. The suffix is
/// *appended*, not substituted, so `data.csv` is tried as `data.csv`, then
/// `data.csv.tex` — the same thing LaTeX does, and it keeps a dotted filename from being
/// silently truncated to its stem.
const EXTENSION_FALLBACKS: [&str; 2] = [".tex", ".latex"];

/// A [`SourceResolver`] that serves files from one directory and refuses everything else.
///
/// Its containment rule is the whole point, so it is worth stating exactly: a resolved
/// file is accepted only if its canonical path starts with the canonical directory
/// **followed by a path separator**. The separator is not decoration. Comparing the bare
/// prefix would accept `/home/user/notes-evil/secret.tex` for a sandbox rooted at
/// `/home/user/notes`, because the one string does start with the other (PLAN.md §14.3
/// names this the `dir-evil` case).
#[derive(Clone, Debug)]
pub struct DirectoryResolver {
    /// The canonical sandbox root. Canonical because every comparison it takes part in
    /// is against a canonical path, and `/proc/self/root/../home` must not be able to
    /// name the same directory by a different spelling.
    root: PathBuf,
    /// `root` with a trailing separator, precomputed: this, not `root`, is what a
    /// contained path must be prefixed by.
    root_prefix: PathBuf,
}

impl DirectoryResolver {
    /// Root a resolver at `directory`.
    ///
    /// The error is the canonicalization failure — the directory does not exist, is not
    /// reachable, or is not a directory — reported as an I/O error the caller turns into
    /// exit code 2. Failing here rather than at the first `\input` is deliberate: a
    /// mistyped `--input-dir` is a mistake in the command, not in the document.
    pub fn new(directory: &Path) -> std::io::Result<DirectoryResolver> {
        let root = directory.canonicalize()?;
        if !root.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                format!("{} is not a directory", directory.display()),
            ));
        }
        let mut prefix = root.clone().into_os_string();
        // The root of the filesystem already ends in a separator; appending a second one
        // would make `/foo` fail to match the prefix `//`.
        if !prefix
            .as_encoded_bytes()
            .ends_with(MAIN_SEPARATOR_STR.as_bytes())
        {
            prefix.push(MAIN_SEPARATOR_STR);
        }
        Ok(DirectoryResolver {
            root,
            root_prefix: PathBuf::from(prefix),
        })
    }

    /// Whether `candidate` — which must already be canonical — lies inside the sandbox.
    ///
    /// The comparison is on the encoded bytes rather than on `Path::starts_with`'s
    /// component walk. Both answer the same question here; the byte prefix is the form
    /// PLAN.md §13 specifies ("dir + separator prefix"), and it is the form whose failure
    /// mode is easy to reason about.
    ///
    /// Note that the root itself is *not* contained — it is a directory, and a directory
    /// is not a document — except when the root is the filesystem root, where the
    /// separator is the whole of it. That exception costs nothing: [`locate`] only ever
    /// offers regular files to this test.
    ///
    /// [`locate`]: DirectoryResolver::locate
    fn contains(&self, candidate: &Path) -> bool {
        candidate
            .as_os_str()
            .as_encoded_bytes()
            .starts_with(self.root_prefix.as_os_str().as_encoded_bytes())
    }

    /// Find the file a reference names, canonicalize it, and check it is in the sandbox.
    ///
    /// Every rejection carries the same shape of message, naming the reference as the
    /// document wrote it: the person reading the diagnostic is reading a document, not a
    /// directory listing.
    fn locate(&self, reference: &str) -> Result<PathBuf, ResolveError> {
        // `join` with an absolute reference discards the root entirely — that is what
        // `\input{/etc/passwd}` is trying to exploit, and it is why containment is
        // checked on the result instead of assumed from the join.
        let direct = self.root.join(reference);
        let mut candidates = Vec::with_capacity(1 + EXTENSION_FALLBACKS.len());
        candidates.push(direct.clone());
        for extension in EXTENSION_FALLBACKS {
            let mut with_extension = direct.clone().into_os_string();
            with_extension.push(extension);
            candidates.push(PathBuf::from(with_extension));
        }

        let mut found = None;
        for candidate in &candidates {
            // Canonicalization is the existence check as well as the normalization: a
            // path that cannot be resolved cannot be read either.
            if let Ok(canonical) = candidate.canonicalize() {
                if canonical.is_file() {
                    found = Some(canonical);
                    break;
                }
            }
        }
        let Some(canonical) = found else {
            return Err(ResolveError::new(
                reference,
                format!(
                    "no such file under {} (tried ‘{}’{})",
                    self.root.display(),
                    reference,
                    EXTENSION_FALLBACKS
                        .iter()
                        .map(|extension| format!(", ‘{reference}{extension}’"))
                        .collect::<String>(),
                ),
            ));
        };

        if !self.contains(&canonical) {
            return Err(ResolveError::new(
                reference,
                format!(
                    "refusing to read {}: it is outside {}",
                    canonical.display(),
                    self.root.display(),
                ),
            ));
        }
        Ok(canonical)
    }
}

impl SourceResolver for DirectoryResolver {
    fn resolve(
        &self,
        reference: &str,
        triggered_at: &SourceSpan,
    ) -> Result<ResolvedContent, ResolveError> {
        let canonical = self.locate(reference)?;

        // The cycle guard walks the provenance chain of the *including* source and
        // compares origins, so it only works because the origin minted below is the
        // canonical path: two spellings of one file have to arrive as one key.
        check_include_chain(
            &canonical,
            triggered_at,
            |origin: &Option<String>| origin.as_ref().map(PathBuf::from),
            Some(MAX_INCLUDE_DEPTH),
        )?;

        let content = std::fs::read_to_string(&canonical)
            .map_err(|error| ResolveError::new(reference, error.to_string()))?;
        Ok(ResolvedContent::new(content)
            .with_origin(Some(canonical.to_string_lossy().into_owned())))
    }
}

/// Build the resolver `--input-dir` asks for, ready for
/// `ConverterBuilder::source_resolver`.
///
/// The `Arc` is what the builder stores anyway; handing one over avoids a second
/// allocation and keeps the resolver cheap to share across the parse.
pub fn directory_resolver(directory: &Path) -> std::io::Result<Arc<DirectoryResolver>> {
    Ok(Arc::new(DirectoryResolver::new(directory)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The containment rule is the security-relevant part, and it is a pure string
    /// question once the paths are canonical — so it is tested as one, without a
    /// filesystem. The end-to-end behaviour (`../` escapes, symlinks, the `dir-evil`
    /// sibling) is exercised through the real binary in `tests/cli_input_dir.rs`.
    fn resolver_at(root: &str) -> DirectoryResolver {
        let root = PathBuf::from(root);
        let mut prefix = root.clone().into_os_string();
        if !prefix
            .as_encoded_bytes()
            .ends_with(MAIN_SEPARATOR_STR.as_bytes())
        {
            prefix.push(MAIN_SEPARATOR_STR);
        }
        DirectoryResolver {
            root,
            root_prefix: PathBuf::from(prefix),
        }
    }

    #[test]
    fn a_file_inside_the_directory_is_contained() {
        let resolver = resolver_at("/srv/paper");
        assert!(resolver.contains(Path::new("/srv/paper/intro.tex")));
        assert!(resolver.contains(Path::new("/srv/paper/chapters/one.tex")));
    }

    #[test]
    fn a_sibling_sharing_the_name_prefix_is_not() {
        // PLAN.md §14.3's `dir-evil` case: the bare-prefix comparison accepts this, the
        // separator-terminated one does not.
        let resolver = resolver_at("/srv/paper");
        assert!(!resolver.contains(Path::new("/srv/paper-evil/secret.tex")));
        assert!(!resolver.contains(Path::new("/srv/paperwork/secret.tex")));
    }

    #[test]
    fn the_directory_itself_and_its_parents_are_not_contained() {
        let resolver = resolver_at("/srv/paper");
        assert!(!resolver.contains(Path::new("/srv/paper")));
        assert!(!resolver.contains(Path::new("/srv")));
        assert!(!resolver.contains(Path::new("/etc/passwd")));
    }

    #[test]
    fn a_root_directory_does_not_grow_a_second_separator() {
        // `--input-dir /` is a sandbox around the whole filesystem, which is a strange
        // thing to ask for but must not be a *broken* thing to ask for: appending a
        // separator to a root that already ends in one would make the prefix `//` and
        // reject every path there is.
        let resolver = resolver_at("/");
        assert!(resolver.contains(Path::new("/etc/passwd")));
        assert!(resolver.contains(Path::new("/paper/intro.tex")));
        // The one case where the root itself passes the prefix test, since the separator
        // *is* the root. Harmless: `locate` only ever offers regular files.
        assert!(resolver.contains(Path::new("/")));
    }
}
