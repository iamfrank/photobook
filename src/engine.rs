//! The official embedded Typst compiler and its file/font environment.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow};
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::datetime::Time;
use typst_kit::diagnostics::{self, DiagnosticFormat, DiagnosticWorld, termcolor::NoColor};
use typst_kit::files::{FileLoader, FileStore, FsRoot};
use typst_kit::fonts::{self, FontStore};
use typst_layout::PagedDocument;

static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::default()));
static FONTS: LazyLock<FontStore> = LazyLock::new(|| {
    let mut store = FontStore::new();
    store.extend(fonts::embedded());
    store.extend(fonts::system());
    store
});

pub(crate) struct Typeset {
    pub document: PagedDocument,
    pub warnings: Vec<String>,
    world: CompilerWorld,
}

pub(crate) fn typeset(source: &Path) -> Result<Typeset> {
    let source = source.canonicalize().context("Cannot find Typst source")?;
    let root = FsRoot::new(
        source
            .parent()
            .context("Source has no parent directory")?
            .to_owned(),
    );
    let path = VirtualPath::virtualize(root.path(), &source)?;
    let main = RootedPath::new(VirtualRoot::Project, path).intern();
    let time = match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(value) => Time::fixed_timestamp(value.parse().context("Invalid SOURCE_DATE_EPOCH")?)
            .map_err(|error| anyhow!("Invalid SOURCE_DATE_EPOCH: {error}"))?,
        Err(_) => Time::system(),
    };
    let world = CompilerWorld {
        main,
        files: FileStore::new(ProjectFiles(root)),
        time,
    };
    let result = typst::compile::<PagedDocument>(&world);
    let warnings = result
        .warnings
        .iter()
        .map(|warning| format_diagnostics(&world, std::slice::from_ref(warning)))
        .collect();
    let document = result.output.map_err(|errors| {
        anyhow!(
            "Typst compilation failed:\n{}",
            format_diagnostics(&world, &errors)
        )
    })?;
    Ok(Typeset {
        document,
        warnings,
        world,
    })
}

impl Typeset {
    pub fn pdf(&self, document: &PagedDocument, booklet: bool) -> Result<Vec<u8>> {
        let options = typst_pdf::PdfOptions {
            tagged: !booklet,
            ..Default::default()
        };
        typst_pdf::pdf(document, &options).map_err(|errors| {
            anyhow!(
                "Typst PDF export failed:\n{}",
                format_diagnostics(&self.world, &errors)
            )
        })
    }
}

struct ProjectFiles(FsRoot);

impl FileLoader for ProjectFiles {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        match id.root() {
            VirtualRoot::Project => self.0.load(id.vpath()),
            VirtualRoot::Package(_) => Err(FileError::Other(Some(
                "Package imports are not supported; use local Typst files within the source directory".into(),
            ))),
        }
    }
}

struct CompilerWorld {
    main: FileId,
    files: FileStore<ProjectFiles>,
    time: Time,
}

impl World for CompilerWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }
    fn book(&self) -> &LazyHash<FontBook> {
        FONTS.book()
    }
    fn main(&self) -> FileId {
        self.main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.source(id)
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }
    fn font(&self, index: usize) -> Option<Font> {
        FONTS.font(index)
    }
    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.time.today(offset)
    }
}

impl DiagnosticWorld for CompilerWorld {
    fn name(&self, id: FileId) -> String {
        match id.root() {
            VirtualRoot::Project => self
                .files
                .loader()
                .0
                .resolve(id.vpath())
                .map(|path: PathBuf| path.display().to_string())
                .unwrap_or_else(|_| id.vpath().get_with_slash().to_owned()),
            VirtualRoot::Package(package) => format!("{package}{}", id.vpath().get_with_slash()),
        }
    }
}

fn format_diagnostics(world: &CompilerWorld, diagnostics: &[SourceDiagnostic]) -> String {
    let mut output = NoColor::new(Vec::new());
    if diagnostics::emit(&mut output, world, diagnostics, DiagnosticFormat::Human).is_ok() {
        String::from_utf8_lossy(&output.into_inner())
            .trim_end()
            .to_owned()
    } else {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests;
