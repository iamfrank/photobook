use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, metadata::Orientation};
use tempfile::{NamedTempFile, tempdir_in};

#[derive(Debug)]
pub struct Generation {
    pub source: PathBuf,
    pub images: usize,
    pub pages: usize,
    pub warnings: Vec<String>,
}

/// Generate an A5 source with one photo per page and a sibling `<stem>.assets` snapshot folder.
/// Edit the generated Typst source to group more photos on particular pages.
/// Neither existing sources nor asset folders are overwritten.
pub fn generate(folder: &Path, output: &Path) -> Result<Generation> {
    ensure!(folder.is_dir(), "Not an image folder: {}", folder.display());
    ensure!(
        output
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("typ")),
        "Output must end in .typ"
    );
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .context("Output directory must exist")?;
    let output = parent.join(output.file_name().context("Missing output filename")?);
    let stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Output filename must be UTF-8")?;
    let assets_name = format!("{stem}.assets");
    let assets = parent.join(&assets_name);
    ensure!(
        !output.try_exists()?,
        "Source already exists: {}; choose a new output name to preserve reviewed edits",
        output.display()
    );
    ensure!(
        !assets.try_exists()?,
        "Asset folder already exists: {}; choose a new output name",
        assets.display()
    );

    let mut images = Vec::new();
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Image folder filenames must be UTF-8")?;
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !name.starts_with('.')
            && matches!(
                extension.as_str(),
                "jpg" | "jpeg" | "png" | "tif" | "tiff" | "webp" | "bmp"
            )
        {
            images.push((name.to_owned(), path));
        }
    }
    images.sort_by(|a, b| natord::compare_ignore_case(&a.0, &b.0).then(a.0.cmp(&b.0)));
    ensure!(
        !images.is_empty(),
        "No supported images found in {}",
        folder.display()
    );

    let staging = tempdir_in(&parent)?;
    let staged_assets = staging.path().join(&assets_name);
    fs::create_dir(&staged_assets)?;
    let mut calls = Vec::new();
    let mut warnings = Vec::new();
    for (index, (name, path)) in images.iter().enumerate() {
        let (asset, width, height) = prepare_image(path, &staged_assets, index + 1)
            .with_context(|| format!("Cannot prepare image {name}"))?;
        calls.push(format!(
            "  // Original: {}\n  photo({}, caption: {}),\n",
            typst_string(name),
            typst_string(&format!("{assets_name}/{asset}")),
            typst_string(&format!("Caption {index}"))
        ));
        let mm_per_pixel = (124.0 / f64::from(width)).min(186.0 / f64::from(height));
        let dpi = 25.4 / mm_per_pixel;
        if dpi < 150.0 {
            warnings.push(format!(
                "{name}: estimated {dpi:.0} DPI; check print quality (aim for 300 DPI)"
            ));
        }
    }

    let mut source = String::from(include_str!("template.typ"));
    for (index, call) in calls.iter().enumerate() {
        if index > 0 {
            source.push_str("#pagebreak()\n\n");
        }
        source.push_str("#book-page(\n");
        source.push_str(call);
        source.push_str(")\n\n");
    }
    let mut staged_source = NamedTempFile::new_in(&parent)?;
    staged_source.write_all(source.as_bytes())?;
    fs::rename(&staged_assets, &assets).context("Could not publish image assets")?;
    if let Err(error) = staged_source.persist_noclobber(&output) {
        let _ = fs::remove_dir_all(&assets);
        return Err(error.error).context("Could not publish Typst source");
    }
    Ok(Generation {
        source: output,
        images: images.len(),
        pages: images.len(),
        warnings,
    })
}

fn prepare_image(path: &Path, assets: &Path, index: usize) -> Result<(String, u32, u32)> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let format = reader.format();
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let (width, height) = (image.width(), image.height());
    let extension = match (format, orientation) {
        (Some(ImageFormat::Jpeg), Orientation::NoTransforms) => "jpg",
        (Some(ImageFormat::Png), Orientation::NoTransforms) => "png",
        _ => "png",
    };
    let filename = format!("{index:04}.{extension}");
    let destination = assets.join(&filename);
    if orientation == Orientation::NoTransforms
        && matches!(format, Some(ImageFormat::Jpeg | ImageFormat::Png))
    {
        fs::copy(path, &destination)?;
    } else {
        // PNG keeps normalized/oriented pixels losslessly, without JPEG re-encoding.
        image.save_with_format(&destination, ImageFormat::Png)?;
    }
    Ok((filename, width, height))
}

/// Typst string escaping is not JSON escaping (control escapes use \u{...}).
pub(crate) fn typst_string(text: &str) -> String {
    let mut output = String::from("\"");
    for character in text.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c.is_control() => {
                write!(output, "\\u{{{:x}}}", c as u32).unwrap();
            }
            c => output.push(c),
        }
    }
    output.push('"');
    output
}
