use std::fs;

use super::*;
use crate::{booklet, generate};
use image::{ImageBuffer, Rgb};
use tempfile::tempdir;
use typst::layout::Abs;
use typst::model::Document;
use typst::utils::Scalar;
use typst_render::{RenderOptions, render};

#[test]
fn local_imports_and_file_cache_refresh_between_compilations() -> Result<()> {
    let temp = tempdir()?;
    let source = temp.path().join("book.typ");
    fs::write(
        &source,
        "#import \"config.typ\": title\n#set document(title: title)\n#datetime.today().display()",
    )?;
    fs::write(temp.path().join("config.typ"), "#let title = \"First\"")?;
    assert_eq!(
        typeset(&source)?.document.info().title.as_deref(),
        Some("First")
    );
    fs::write(temp.path().join("config.typ"), "#let title = \"Reviewed\"")?;
    assert_eq!(
        typeset(&source)?.document.info().title.as_deref(),
        Some("Reviewed")
    );
    Ok(())
}

#[test]
fn compiler_diagnostics_and_unsupported_packages_are_clear() -> Result<()> {
    let temp = tempdir()?;
    let source = temp.path().join("book.typ");
    fs::write(&source, "#image(\"missing.png\")")?;
    let error = typeset(&source).err().unwrap().to_string();
    assert!(error.contains("missing.png"));
    assert!(error.contains("book.typ:1"));
    fs::write(&source, "#import \"@preview/example:0.1.0\": *")?;
    assert!(
        typeset(&source)
            .err()
            .unwrap()
            .to_string()
            .contains("Package imports are not supported")
    );
    fs::write(
        &source,
        "#set text(font: \"photobook-nonexistent-font\")\nHello",
    )?;
    let compiled = typeset(&source)?;
    assert!(
        compiled
            .warnings
            .iter()
            .any(|warning| warning.contains("photobook-nonexistent-font"))
    );
    Ok(())
}

#[test]
fn rows_are_centered_and_each_page_uses_its_own_photo_count() -> Result<()> {
    let temp = tempdir()?;
    let photos = temp.path().join("photos");
    fs::create_dir(&photos)?;
    for number in 1..=7 {
        ImageBuffer::from_pixel(300, 200, Rgb([40u8, 120, 200]))
            .save(photos.join(format!("{number}.jpg")))?;
    }
    let options = RenderOptions {
        pixel_per_pt: Scalar::new(1.0),
        ..Default::default()
    };
    let source = temp.path().join("grid.typ");
    generate(&photos, &source)?;
    let mut text = fs::read_to_string(&source)?;
    // Empty captions isolate photo placement from caption height.
    for index in 0..7 {
        text = text.replace(&format!("caption: \"Caption {index}\""), "caption: \"\"");
    }
    let boundary = ")\n\n#pagebreak()\n\n#book-page(\n";
    let groups: Vec<_> = text.split(boundary).collect();
    assert_eq!(groups.len(), 7);
    for group_sizes in [vec![1; 7], vec![2, 2, 2, 1], vec![3, 3, 1], vec![2, 4, 1]] {
        let mut offset = 0;
        let edited = group_sizes
            .iter()
            .map(|&size| {
                let group = groups[offset..offset + size].concat();
                offset += size;
                group
            })
            .collect::<Vec<_>>()
            .join(boundary);
        fs::write(&source, edited)?;
        let compiled = typeset(&source)?;
        let pages = compiled.document.pages();
        assert_eq!(pages.len(), group_sizes.len());
        for (index, (page, &count)) in pages.iter().zip(&group_sizes).enumerate() {
            let row_mm = (186.0 - 5.0 * (count as f64 - 1.0)) / count as f64;
            let photo_mm = (124.0_f64 / 1.5).min(row_mm);
            let expected_top = (12.0 + (row_mm - photo_mm) / 2.0) * 72.0 / 25.4;
            assert_eq!(page.frame.width(), Abs::mm(148.0));
            assert_eq!(page.frame.height(), Abs::mm(210.0));
            let rendered = render(page, &options);
            let x = rendered.width() / 2;
            let is_photo = |y| {
                let pixel = rendered.pixel(x, y).unwrap();
                pixel.red() < 70 && pixel.green() > 90 && pixel.green() < 150 && pixel.blue() > 150
            };
            let top = (0..rendered.height())
                .find(|&y| is_photo(y))
                .expect("visible photo");
            let bottom = (top..rendered.height())
                .find(|&y| !is_photo(y))
                .unwrap_or(rendered.height());
            assert!(
                (f64::from(top) - expected_top).abs() < 2.0,
                "photos={count}, page={index}, top={top}, expected={expected_top}"
            );
            assert!((f64::from(bottom - top) - photo_mm * 72.0 / 25.4).abs() < 2.0);
        }
    }
    Ok(())
}

#[test]
fn booklet_clips_overflow_and_retains_vector_text() -> Result<()> {
    let temp = tempdir()?;
    let source = temp.path().join("book.typ");
    fs::write(
        &source,
        "#set page(paper: \"a5\", margin: 0pt)\n#place(top + left, dx: -100mm, rect(width: 350mm, height: 210mm, fill: red, stroke: none))\nHello Ærø",
    )?;
    let compiled = typeset(&source)?;
    let (document, _) = booklet::impose(&compiled.document)?;
    let sheet = render(
        &document.pages()[0],
        &RenderOptions {
            pixel_per_pt: Scalar::new(1.0),
            ..Default::default()
        },
    );
    let white = sheet.pixel(50, 200).unwrap();
    assert_eq!((white.red(), white.green(), white.blue()), (255, 255, 255));
    let red = sheet.pixel(600, 200).unwrap();
    assert!(red.red() > 200 && red.green() < 100);
    let bytes = compiled.pdf(&document, true)?;
    // Native Typst text remains text/font data, not an image-only PDF.
    assert!(
        bytes
            .windows(b"/Font".len())
            .any(|window| window == b"/Font")
    );
    Ok(())
}
