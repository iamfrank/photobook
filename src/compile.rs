use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use tempfile::NamedTempFile;

use crate::booklet::{self, BookletStats};
use crate::engine;

#[derive(Debug)]
pub struct Compilation {
    pub output: PathBuf,
    /// Number of pages in the exported PDF (sheet sides for a booklet).
    pub pages: usize,
    pub booklet: Option<BookletStats>,
    pub warnings: Vec<String>,
}

/// Compile reviewed source. Existing PDFs are replaced only on success.
/// With no explicit output, use <stem>.pdf or <stem>-booklet.pdf.
pub fn compile(source: &Path, output: Option<&Path>, booklet: bool) -> Result<Compilation> {
    ensure!(
        source
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("typ")),
        "Input must end in .typ"
    );
    let source = source.canonicalize().context("Cannot find Typst source")?;
    ensure!(
        source.is_file(),
        "Not a Typst source file: {}",
        source.display()
    );
    let default = if booklet {
        source.with_file_name(format!(
            "{}-booklet.pdf",
            source.file_stem().unwrap().to_string_lossy()
        ))
    } else {
        source.with_extension("pdf")
    };
    let output = output.unwrap_or(&default);
    ensure!(
        output
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")),
        "Only PDF output is supported; output must end in .pdf (HTML is deferred)"
    );
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .context("Output directory must exist")?;
    let output = parent.join(output.file_name().context("Missing output filename")?);
    ensure!(
        fs::canonicalize(&output).ok().as_ref() != Some(&source),
        "Output must not overwrite the Typst source"
    );

    let typeset = engine::typeset(&source)?;
    let (document, stats) = if booklet {
        let (document, stats) = booklet::impose(&typeset.document)?;
        (document, Some(stats))
    } else {
        (typeset.document.clone(), None)
    };
    let pdf = typeset.pdf(&document, booklet)?;
    let mut temporary = NamedTempFile::new_in(&parent)?;
    temporary.write_all(&pdf)?;
    temporary
        .persist(&output)
        .context("Could not publish PDF")?;
    Ok(Compilation {
        output,
        pages: document.pages().len(),
        booklet: stats,
        warnings: typeset.warnings,
    })
}
