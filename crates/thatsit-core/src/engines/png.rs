//! `builtin:optimize-png` — lossless PNG optimisation with `oxipng`.
//!
//! Args: `level` (0–6, default 2; higher is slower and slightly smaller).

use oxipng::{InFile, Options, OutFile};

use super::{arg_int, io_err};
use crate::pipeline::StepContext;

pub(super) fn run(args: &toml::Table, ctx: &StepContext) -> Result<(), String> {
    let level = arg_int(args, "level", 0..=6)?.unwrap_or(2) as u8;
    let options = Options::from_preset(level);
    oxipng::optimize(
        &InFile::Path(ctx.input.clone()),
        &OutFile::Path {
            path: Some(ctx.output.clone()),
            preserve_attrs: false,
        },
        &options,
    )
    .map(|_| ())
    .map_err(|e| io_err("cannot optimize the PNG", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::test_util::{ctx, table};

    #[test]
    fn optimizes_losslessly() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.png");
        let mut img = image::RgbaImage::new(64, 64);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x * 4) as u8, (y * 4) as u8, 0, 255]);
        }
        img.save(&input).unwrap();
        let out = dir.path().join("out.png");
        run(&table("level = 1"), &ctx(&input, &out)).unwrap();
        assert_eq!(image::open(&out).unwrap().to_rgba8(), img);
        assert!(run(&table("level = 9"), &ctx(&input, &out)).is_err());
    }
}
