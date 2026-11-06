//! `builtin:svg-to-png` — renders SVG with `resvg`.
//!
//! Args: `scale` (0.1–16, default 2: renders at twice the SVG's nominal size, which suits
//! Retina displays). System fonts are loaded so `<text>` renders.

use resvg::{tiny_skia, usvg};

use super::{arg_float, io_err};
use crate::pipeline::StepContext;

/// Refuse to allocate pixmaps above this many pixels (≈ 256 MB of RGBA).
const MAX_PIXELS: u64 = 64_000_000;

pub(super) fn run(args: &toml::Table, ctx: &StepContext) -> Result<(), String> {
    let scale = arg_float(args, "scale", 0.1..=16.0)?.unwrap_or(2.0) as f32;
    let data = std::fs::read(&ctx.input).map_err(|e| io_err("cannot read the SVG", e))?;

    let mut options = usvg::Options {
        resources_dir: ctx.input.parent().map(|p| p.to_path_buf()),
        ..usvg::Options::default()
    };
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(&data, &options).map_err(|e| io_err("invalid SVG", e))?;

    let size = tree
        .size()
        .to_int_size()
        .scale_by(scale)
        .ok_or("the SVG has an invalid size")?;
    if size.width() as u64 * size.height() as u64 > MAX_PIXELS {
        return Err(format!(
            "the rendered image would be too large ({}×{})",
            size.width(),
            size.height()
        ));
    }
    let mut pixmap =
        tiny_skia::Pixmap::new(size.width(), size.height()).ok_or("cannot allocate the image")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .save_png(&ctx.output)
        .map_err(|e| io_err("cannot write the PNG", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::test_util::{ctx, table};

    #[test]
    fn renders_at_scale() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.svg");
        std::fs::write(
            &input,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="20"><rect width="30" height="20" fill="#f00"/></svg>"##,
        )
        .unwrap();
        let out = dir.path().join("a.png");
        run(&table(""), &ctx(&input, &out)).unwrap();
        let img = image::open(&out).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (60, 40));
        assert_eq!(img.get_pixel(30, 20).0, [255, 0, 0, 255]);

        run(&table("scale = 1"), &ctx(&input, &out)).unwrap();
        assert_eq!(image::open(&out).unwrap().width(), 30);
    }

    #[test]
    fn rejects_invalid_svg() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.svg");
        std::fs::write(&input, "not svg").unwrap();
        assert!(run(&table(""), &ctx(&input, &dir.path().join("o.png"))).is_err());
    }
}
