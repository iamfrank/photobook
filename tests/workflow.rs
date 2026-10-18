use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use image::{ImageBuffer, Rgb, Rgba};
use photobook::{compile, generate};
use tempfile::tempdir;

fn photo(folder: &Path, name: &str, width: u32, height: u32) {
    ImageBuffer::from_pixel(width, height, Rgb([40u8, 120, 200]))
        .save(folder.join(name))
        .unwrap();
}

fn assert_pdf(path: &Path) -> Result<()> {
    let bytes = fs::read(path)?;
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        String::from_utf8_lossy(&bytes)
            .trim_end()
            .ends_with("%%EOF")
    );
    Ok(())
}

#[test]
fn natural_order_placeholder_captions_snapshots_and_page_groups() -> Result<()> {
    let temp = tempdir()?;
    let photos = temp.path().join("photos");
    fs::create_dir(&photos)?;
    for name in ["photo10.jpg", "photo2.JPG", "photo1.png"] {
        photo(&photos, name, 300, 200);
    }
    photo(&photos, ".hidden.png", 10, 10);
    fs::create_dir(photos.join("nested"))?;
    photo(&photos.join("nested"), "ignored.png", 10, 10);
    fs::write(photos.join("notes.txt"), "ignored")?;
    let output = temp.path().join("my book.typ");
    let result = generate(&photos, &output)?;
    assert_eq!((result.images, result.pages), (3, 3));
    let text = fs::read_to_string(&output)?;
    assert!(text.find("photo1.png").unwrap() < text.find("photo2.JPG").unwrap());
    assert!(text.find("photo2.JPG").unwrap() < text.find("photo10.jpg").unwrap());
    for index in 0..3 {
        assert!(text.contains(&format!("caption: \"Caption {index}\"")));
    }
    assert_eq!(text.matches("#book-page(").count(), 3);
    assert_eq!(text.matches("#pagebreak()").count(), 2);
    assert!(!text.contains("images-per-page"));
    for page in text.split("#book-page(").skip(1) {
        assert_eq!(page.matches("  photo(").count(), 1);
    }
    assert!(!text.contains("hidden"));
    assert_eq!(
        fs::read(temp.path().join("my book.assets/0002.jpg"))?,
        fs::read(photos.join("photo2.JPG"))?
    );
    fs::remove_dir_all(photos)?;
    // Compile from the snapshot, even with the entire original folder gone.
    let compiled = compile(&output, None, false)?;
    assert_eq!(compiled.pages, 3);
    assert_pdf(&compiled.output)
}

#[test]
fn protects_reviewed_sources_and_existing_assets() -> Result<()> {
    let temp = tempdir()?;
    photo(temp.path(), "photo.jpg", 30, 20);
    let output = temp.path().join("book.typ");
    generate(temp.path(), &output)?;
    fs::write(&output, "reviewed edits")?;
    assert!(generate(temp.path(), &output).is_err());
    assert_eq!(fs::read_to_string(&output)?, "reviewed edits");
    fs::remove_file(&output)?;
    assert!(generate(temp.path(), &output).is_err());
    assert!(temp.path().join("book.assets/0001.jpg").is_file());
    Ok(())
}

#[test]
fn corrupt_image_never_publishes_partial_book() -> Result<()> {
    let temp = tempdir()?;
    photo(temp.path(), "1.jpg", 30, 20);
    fs::write(temp.path().join("2.jpg"), "not an image")?;
    let output = temp.path().join("book.typ");
    assert!(
        generate(temp.path(), &output)
            .unwrap_err()
            .to_string()
            .contains("2.jpg")
    );
    assert!(!output.exists());
    assert!(!temp.path().join("book.assets").exists());
    assert_eq!(fs::read_dir(temp.path())?.count(), 2);
    Ok(())
}

#[test]
fn validates_input_and_output_paths() -> Result<()> {
    let temp = tempdir()?;
    let output = temp.path().join("book.typ");
    assert!(generate(temp.path(), &output).is_err());
    assert!(generate(&temp.path().join("missing"), &output).is_err());
    photo(temp.path(), "photo.png", 30, 20);
    assert!(generate(temp.path(), &temp.path().join("bad.pdf")).is_err());
    assert!(generate(temp.path(), &temp.path().join("missing/book.typ")).is_err());
    assert!(!output.exists());
    assert!(!temp.path().join("book.assets").exists());
    Ok(())
}

#[test]
fn supported_formats_and_alpha() -> Result<()> {
    let temp = tempdir()?;
    for extension in ["jpg", "png", "tiff", "webp", "bmp"] {
        photo(temp.path(), &format!("photo.{extension}"), 30, 20);
    }
    ImageBuffer::from_pixel(30, 20, Rgba([255u8, 0, 0, 0]))
        .save(temp.path().join("transparent.png"))?;
    let result = generate(temp.path(), &temp.path().join("book.typ"))?;
    assert_eq!((result.images, result.pages), (6, 6));
    assert_eq!(fs::read_dir(temp.path().join("book.assets"))?.count(), 6);
    let alpha = image::open(temp.path().join("book.assets/0006.png"))?.to_rgba8();
    assert_eq!(alpha.get_pixel(0, 0)[3], 0);
    assert_pdf(&compile(&result.source, None, false)?.output)
}

#[test]
fn exif_orientation_is_normalized_without_lossy_reencoding() -> Result<()> {
    let temp = tempdir()?;
    photo(temp.path(), "photo.jpg", 120, 80);
    let original = fs::read(temp.path().join("photo.jpg"))?;
    let exif = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    let mut oriented = vec![0xff, 0xd8, 0xff, 0xe1];
    oriented.extend(((exif.len() + 2) as u16).to_be_bytes());
    oriented.extend(exif);
    oriented.extend(&original[2..]);
    fs::write(temp.path().join("photo.jpg"), oriented)?;
    generate(temp.path(), &temp.path().join("book.typ"))?;
    assert_eq!(
        image::image_dimensions(temp.path().join("book.assets/0001.png"))?,
        (80, 120)
    );
    Ok(())
}

#[test]
fn resolution_estimates_for_one_photo_per_page() -> Result<()> {
    let temp = tempdir()?;
    photo(temp.path(), "1-small.jpg", 30, 20);
    photo(temp.path(), "2-large.jpg", 2400, 1600);
    let result = generate(temp.path(), &temp.path().join("book.typ"))?;
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("1-small.jpg"));
    assert_eq!(result.pages, 2);
    Ok(())
}

#[test]
fn cli_help_validation_and_compilation_without_typst_on_path() -> Result<()> {
    let binary = env!("CARGO_BIN_EXE_photobook");
    let help = Command::new(binary).arg("--help").output()?;
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("generate"));
    let help = Command::new(binary).args(["generate", "--help"]).output()?;
    assert!(help.status.success());
    assert!(!String::from_utf8_lossy(&help.stdout).contains("--per-page"));
    for value in ["1", "2", "3"] {
        let bad = Command::new(binary)
            .args(["generate", ".", "--per-page", value])
            .output()?;
        assert!(!bad.status.success());
        assert!(String::from_utf8_lossy(&bad.stderr).contains("unexpected argument '--per-page'"));
    }
    let temp = tempdir()?;
    fs::write(
        temp.path().join("book.typ"),
        "#set page(paper: \"a5\")\nHello",
    )?;
    for booklet in [false, true] {
        let mut command = Command::new(binary);
        command.args(["compile", "book.typ"]);
        if booklet {
            command.arg("--booklet");
        }
        let result = command
            .current_dir(temp.path())
            .env("PATH", temp.path())
            .output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_pdf(&temp.path().join(if booklet {
            "book-booklet.pdf"
        } else {
            "book.pdf"
        }))?;
    }
    Ok(())
}

#[test]
fn per_page_typst_edits_compile_with_embedded_typst() -> Result<()> {
    let temp = tempdir()?;
    let photos = temp.path().join("photos");
    fs::create_dir(&photos)?;
    for index in 1..=7 {
        photo(
            &photos,
            &format!("photo{index}.jpg"),
            if index % 2 == 0 { 100 } else { 300 },
            200,
        );
    }
    for (index, group_sizes) in [vec![1; 7], vec![2, 2, 2, 1], vec![3, 3, 1], vec![2, 4, 1]]
        .iter()
        .enumerate()
    {
        let source = temp.path().join(format!("book-{index}.typ"));
        let generated = generate(&photos, &source)?;
        assert_eq!((generated.images, generated.pages), (7, 7));
        let text = fs::read_to_string(&source)?
            .replace("Caption 0", "Ærø <beach> #literal [text]\\nLine two")
            .replace("Caption 6", "Reviewed description");
        // Move photo calls between groups by removing selected group boundaries.
        let boundary = ")\n\n#pagebreak()\n\n#book-page(\n";
        let groups: Vec<_> = text.split(boundary).collect();
        assert_eq!(groups.len(), 7);
        let mut offset = 0;
        let text = group_sizes
            .iter()
            .map(|&size| {
                let group = groups[offset..offset + size].concat();
                offset += size;
                group
            })
            .collect::<Vec<_>>()
            .join(boundary);
        fs::write(&source, &text)?;
        let result = compile(&source, None, false)?;
        let pages = group_sizes.len();
        assert_eq!(result.pages, pages);
        assert_pdf(&result.output)?;
        assert_eq!(fs::read_to_string(&source)?, text);
        let booklet = compile(&source, None, true)?;
        let stats = booklet.booklet.unwrap();
        assert_eq!(stats.pages, pages);
        assert_eq!(booklet.pages, pages.div_ceil(4) * 2);
        assert_pdf(&booklet.output)?;
    }
    Ok(())
}

#[test]
fn failed_compilation_and_imposition_preserve_existing_pdf() -> Result<()> {
    let temp = tempdir()?;
    let source = temp.path().join("book.typ");
    let output = temp.path().join("book.pdf");
    fs::write(&output, "existing PDF")?;
    fs::write(&source, "#undefined-function()")?;
    let error = compile(&source, Some(&output), false)
        .unwrap_err()
        .to_string();
    assert!(error.contains("undefined-function"));
    assert!(error.contains("book.typ:1"));
    assert_eq!(fs::read_to_string(&output)?, "existing PDF");
    fs::write(&source, "#set page(paper: \"a4\")\nWrong size")?;
    assert!(
        compile(&source, Some(&output), true)
            .unwrap_err()
            .to_string()
            .contains("A5 portrait")
    );
    assert_eq!(fs::read_to_string(&output)?, "existing PDF");
    assert_eq!(fs::read_dir(temp.path())?.count(), 2);
    Ok(())
}

#[test]
fn one_page_booklet_keeps_the_wholly_blank_back_side() -> Result<()> {
    let temp = tempdir()?;
    let source = temp.path().join("one.typ");
    fs::write(&source, "#set page(paper: \"a5\")\nOne page")?;
    let result = compile(&source, None, true)?;
    assert_eq!(
        result.booklet.unwrap(),
        photobook::BookletStats {
            pages: 1,
            blanks: 3,
            sheets: 1
        }
    );
    assert_eq!(result.pages, 2);
    assert_pdf(&result.output)
}

#[test]
fn caption_overflow_fails_instead_of_spilling_to_extra_pages() -> Result<()> {
    let temp = tempdir()?;
    photo(temp.path(), "photo.jpg", 300, 200);
    let source = temp.path().join("book.typ");
    generate(temp.path(), &source)?;
    let text = fs::read_to_string(&source)?.replace("Caption 0", &"Long caption\\n".repeat(100));
    fs::write(&source, text)?;
    let error = compile(&source, None, false).unwrap_err().to_string();
    assert!(error.contains("Caption too long"));
    assert!(!source.with_extension("pdf").exists());
    Ok(())
}

#[test]
fn compile_validates_extensions_and_missing_inputs() -> Result<()> {
    let temp = tempdir()?;
    assert!(compile(&temp.path().join("missing.typ"), None, false).is_err());
    let source = temp.path().join("book.typ");
    fs::write(&source, "Hello")?;
    assert!(compile(&source, Some(&temp.path().join("book.html")), false).is_err());
    assert!(compile(&source, Some(&temp.path().join("missing/book.pdf")), false).is_err());
    Ok(())
}
