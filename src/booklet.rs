//! Two-up saddle-stitch composition in Typst's native frame model.
//! No intermediate PDF, rasterization, or PDF-processing library is involved.

use anyhow::{Result, ensure};
use typst::foundations::{Content, Smart};
use typst::layout::{Abs, Frame, FrameItem, GroupItem, Point, Size};
use typst::model::Document;
use typst::syntax::Span;
use typst::visualize::{Curve, Geometry};
use typst_layout::{Page, PagedDocument};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookletStats {
    pub pages: usize,
    pub blanks: usize,
    pub sheets: usize,
}

/// Each pair is left/right on a sheet side, front then back, outermost first.
/// Logical page indices are zero-based. Padding pages are represented by None.
fn order(pages: usize) -> Vec<[Option<usize>; 2]> {
    let padded = pages.div_ceil(4) * 4;
    let mut sides = Vec::new();
    for sheet in 0..padded / 4 {
        for pair in [
            [padded - 1 - 2 * sheet, 2 * sheet],
            [2 * sheet + 1, padded - 2 - 2 * sheet],
        ] {
            sides.push(pair.map(|page| (page < pages).then_some(page)));
        }
    }
    sides
}

pub(crate) fn impose(document: &PagedDocument) -> Result<(PagedDocument, BookletStats)> {
    let pages = document.pages();
    ensure!(!pages.is_empty(), "Cannot impose an empty document");
    ensure!(pages.len() < i32::MAX as usize, "Too many pages");
    for (index, page) in pages.iter().enumerate() {
        let size = page.frame.size();
        ensure!(
            (size.x - Abs::mm(148.0)).to_pt().abs() < 0.1
                && (size.y - Abs::mm(210.0)).to_pt().abs() < 0.1,
            "Booklet requires A5 portrait pages (148 × 210 mm); page {} is {:.1} × {:.1} mm",
            index + 1,
            size.x.to_mm(),
            size.y.to_mm()
        );
        ensure!(
            page.bleed == Default::default(),
            "Booklet does not support bleed (page {})",
            index + 1
        );
    }
    let sides = order(pages.len());
    let stats = BookletStats {
        pages: pages.len(),
        blanks: sides.len() * 2 - pages.len(),
        sheets: sides.len() / 2,
    };
    let mut sheets = Vec::new();
    for (index, pair) in sides.into_iter().enumerate() {
        let mut frame = Frame::hard(Size::new(Abs::mm(297.0), Abs::mm(210.0)));
        for (slot, page) in pair.into_iter().enumerate() {
            let Some(page) = page else {
                continue;
            };
            let page = &pages[page];
            let mut logical = page.frame.clone();
            strip_navigation(&mut logical);
            if let Some(fill) = page.fill_or_transparent() {
                logical.prepend(
                    Point::zero(),
                    FrameItem::Shape(
                        Geometry::Rect(logical.size()).filled(fill),
                        Span::detached(),
                    ),
                );
            }
            let mut group = GroupItem::new(logical);
            group.clip = Some(Curve::rect(page.frame.size()));
            // A4 is 1 mm wider than two A5 pages: leave 0.5 mm at each outside
            // edge, with no scaling and both logical pages meeting at the fold.
            let x = Abs::mm(if slot == 0 { 0.5 } else { 148.5 });
            frame.push(Point::new(x, Abs::zero()), FrameItem::Group(group));
        }
        sheets.push(Page {
            frame,
            bleed: Default::default(),
            fill: Smart::Auto,
            numbering: None,
            supplement: Content::empty(),
            number: (index + 1) as u64,
        });
    }
    Ok((
        PagedDocument::new(sheets.into(), document.info().clone()),
        stats,
    ))
}

/// This is a print derivative, not a reflowed accessible document. Do not leave
/// links, tags, or group parents referring to the original reading-order tree.
fn strip_navigation(frame: &mut Frame) {
    frame.retain(|item| match item {
        FrameItem::Tag(_) | FrameItem::Link(..) => false,
        FrameItem::Group(group) => {
            group.parent = None;
            group.label = None;
            strip_navigation(&mut group.frame);
            true
        }
        _ => true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::utils::Scalar;
    use typst::visualize::Color;
    use typst_render::{RenderOptions, render};

    fn colored_document(count: usize) -> PagedDocument {
        let colors = [
            Color::RED,
            Color::BLUE,
            Color::GREEN,
            Color::YELLOW,
            Color::NAVY,
            Color::TEAL,
            Color::PURPLE,
            Color::ORANGE,
        ];
        let pages = (0..count)
            .map(|index| Page {
                frame: Frame::hard(Size::new(Abs::mm(148.0), Abs::mm(210.0))),
                bleed: Default::default(),
                fill: Smart::Custom(Some(colors[index % colors.len()].clone().into())),
                numbering: None,
                supplement: Content::empty(),
                number: (index + 1) as u64,
            })
            .collect();
        PagedDocument::new(pages, Default::default())
    }

    #[test]
    fn eight_page_order() {
        assert_eq!(
            order(8),
            vec![
                [Some(7), Some(0)],
                [Some(1), Some(6)],
                [Some(5), Some(2)],
                [Some(3), Some(4)]
            ]
        );
    }

    #[test]
    fn padding_and_every_page_once() {
        for count in 1..=100 {
            let sides = order(count);
            assert_eq!(sides.len(), count.div_ceil(4) * 2);
            let mut pages: Vec<_> = sides.into_iter().flatten().flatten().collect();
            pages.sort();
            assert_eq!(pages, (0..count).collect::<Vec<_>>());
        }
        assert_eq!(order(1), vec![[None, Some(0)], [None, None]]);
        assert_eq!(
            order(5),
            vec![
                [None, Some(0)],
                [Some(1), None],
                [None, Some(2)],
                [Some(3), Some(4)]
            ]
        );
    }

    #[test]
    fn native_frames_preserve_geometry_fills_order_and_blank_sides() -> Result<()> {
        let options = RenderOptions {
            pixel_per_pt: Scalar::new(1.0),
            ..Default::default()
        };
        for count in [1, 5, 8] {
            let document = colored_document(count);
            let (booklet, stats) = impose(&document)?;
            assert_eq!(stats.pages, count);
            assert_eq!(booklet.pages().len(), stats.sheets * 2);
            for (page, pair) in booklet.pages().iter().zip(order(count)) {
                assert_eq!(page.frame.size(), Size::new(Abs::mm(297.0), Abs::mm(210.0)));
                let sheet = render(page, &options);
                for (slot, logical) in pair.into_iter().enumerate() {
                    let x = (Abs::mm(if slot == 0 { 74.5 } else { 222.5 }).to_pt()) as u32;
                    let y = Abs::mm(105.0).to_pt() as u32;
                    let actual = sheet.pixel(x, y).unwrap();
                    if let Some(logical) = logical {
                        let original = render(&document.pages()[logical], &options);
                        assert_eq!(
                            actual,
                            original
                                .pixel(original.width() / 2, original.height() / 2)
                                .unwrap()
                        );
                    } else {
                        assert_eq!(actual.red(), 255);
                        assert_eq!(actual.green(), 255);
                        assert_eq!(actual.blue(), 255);
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn rejects_empty_wrong_sized_and_bleed_pages() {
        assert!(impose(&colored_document(0)).is_err());
        let original = colored_document(1);
        let mut page = original.pages()[0].clone();
        page.frame
            .set_size(Size::new(Abs::mm(210.0), Abs::mm(297.0)));
        assert!(
            impose(&PagedDocument::new(vec![page].into(), Default::default()))
                .unwrap_err()
                .to_string()
                .contains("A5 portrait")
        );
        let mut page = original.pages()[0].clone();
        page.bleed.left = Abs::mm(3.0);
        assert!(
            impose(&PagedDocument::new(vec![page].into(), Default::default()))
                .unwrap_err()
                .to_string()
                .contains("bleed")
        );
    }
}
