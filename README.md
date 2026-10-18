# photobook

A small Rust CLI for creating **editable Typst photobooks**, compiling reviewed
sources to PDF, and preparing saddle-stitch booklets for printing.

- A5 portrait pages with 12 mm white margins.
- Generated books start with one photo per page. Edit the Typst source to group
  more photos on particular pages; the template handles layout.
- Photos fit without cropping or stretching, with optional descriptions underneath.
  Each photo/description group is centered within its row.
- PDF first. HTML export is deferred.

## Install

Requires **Rust 1.92+** to build. The repository pins Rust 1.92.0 via
`rust-toolchain.toml`; rustup can install that toolchain automatically.

```sh
cargo install --path . --locked
```

The official **Typst 0.15.1 compiler and PDF exporter are embedded** in the Rust
application (`typst`, `typst-layout`, `typst-pdf`, and `typst-kit`). No separate
Typst executable or PDF-processing tool is needed at runtime.
The initial build is substantial because it includes the compiler and bundled fonts.

## 1. Generate and review

```sh
photobook generate ./photos -o book.typ
```

Generation creates `book.typ` and a sibling **`book.assets/`** directory containing
snapshots of the images. It does **not** run Typst. Review the source, edit captions,
reorder `photo(...)` calls, or move calls between `book-page(...)` groups before
compiling. Generation always starts with one photo per page; there is no global
images-per-page setting. To put multiple photos on a particular page, move their
`photo(...)` calls into the same group and remove the now-empty groups and their
page breaks. The Typst template gives each photo an equal-height row based on
that group's photo count; other pages are unaffected. Explicit page breaks
separate the groups.

For example, edit two generated pages into one:

```typst
#book-page(
  photo("book.assets/0001.jpg", caption: "First photo"),
  photo("book.assets/0002.jpg", caption: "Second photo"),
)
```

The default output is `photobook.typ` in the current directory. Existing `.typ`
files and asset folders are **never overwritten**, protecting reviewed edits.
Choose a new output name to regenerate. The output directory must already exist.
Keep the `.typ` file and its `.assets` folder together when moving/sharing a book.
Image edits in the original photo folder do not affect an already generated book.

### Images and descriptions

Supported: JPEG, PNG, TIFF, WebP, BMP. Only immediate, non-hidden files are included;
subfolders are ignored. Multi-frame images use their first frame. Filenames sort
naturally and case-insensitively: `photo1.jpg`, `photo2.jpg`, `photo10.jpg`. Prefix
filenames with numbers to choose the initial order.

EXIF orientation is applied during generation. Unchanged JPEG/PNG files are copied
without re-encoding; other formats and reoriented images are normalized to lossless
PNG assets. Transparency prints on white paper. Images are not resampled. Large
normalized photos can produce large asset folders/PDFs. Generation warns below an
estimated 150 DPI. Aim for 300 DPI.
After editing the layout, check image resolution again yourself.

## 2. Compile the reviewed source

```sh
photobook compile book.typ                 # → book.pdf
photobook compile book.typ -o final.pdf
```

Uses the embedded official Typst compiler and `typst-pdf` exporter. It does not
regenerate source or require the original image folder.
Warnings and errors include source locations. An existing output PDF is replaced
only after compilation and export both succeed.

Local Typst imports and image/data files are supported within the source file's
directory tree. Package imports (`@preview/...`) and automatic network downloads
are deliberately unsupported in this minimal version. Compilation works offline.
`datetime.today()` is supported; `SOURCE_DATE_EPOCH` can fix its date for repeatable
builds. CLI-specific `TYPST_*` environment settings are not used.

For live preview, you may **optionally** install the separate Typst CLI and run
`typst watch book.typ`. The generated source remains ordinary, editable Typst.

Only compile trusted sources. File loading is rooted at the source directory but
can follow symlinks; this is not a sandbox for untrusted documents.

## 3. Print a saddle-stitch booklet

```sh
photobook compile book.typ --booklet       # → book-booklet.pdf
photobook compile book.typ --booklet -o print.pdf
```

This uses **Typst's native document/frame model and official PDF exporter**:

1. Compile the reviewed source into an in-memory Typst `PagedDocument`.
2. Validate its A5 portrait pages and arrange their frames onto A4 landscape sheet
   sides in saddle-stitch order, clipping each logical page to its original bounds.
3. Export the composed document with `typst-pdf`.

There are no intermediate PDFs, PDF parsers, subprocesses, or SVG/raster conversion
steps. Every logical page must be A5 portrait (148 × 210 mm), without bleed.
Output is an ordinary PDF with a special page arrangement—not a new file standard.

Blank pages are appended to make the logical page count a multiple of four. For
eight pages, the PDF sides are:

| Sheet | Front, left / right | Back, left / right |
|---|---|---|
| Outer | 8 / 1 | 2 / 7 |
| Inner | 6 / 3 | 4 / 5 |

PDF pages are sheet fronts/backs, outermost sheet first. All are upright. Original
text, fonts, images, vector graphics, and page fills remain in Typst's native frame
model. This is a print derivative: logical-page links, navigation, and accessibility
tagging are removed rather than retaining the original reading-order tree.

### Printer settings

- **A4, landscape, actual size / 100%** (disable fit-to-page).
- **Duplex, flip on short edge.** Verify with one test sheet; printer drivers vary.
- **One PDF page per sheet side.** Do not enable booklet printing or another
  multiple-pages-per-sheet option: the PDF is already imposed.
- Nest the sheets with the outer sheet outside, fold down the center, then staple.
  The first logical page is the front cover; add your own covers/intentional blanks
  to the reviewed source if desired.

Two exact A5 pages span 296 mm; A4 is 297 mm wide. The extra 1 mm is placed at the
outside edges (0.5 mm each), keeping pages at 100% size and meeting at the fold.
Trim the outside excess if exact A5 dimensions matter.

No creep compensation, bleed, crop marks, CMYK/ICC conversion, PDF/X guarantee,
or sewn-signature support. For thick books, use a printer's imposition/preflight
workflow. Check commercial printer requirements before ordering.

## Development

```sh
cargo test --locked                        # All tests use the embedded compiler
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```
