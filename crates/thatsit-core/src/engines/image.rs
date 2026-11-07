//! `builtin:image` — raster format conversion and resizing with the `image` crate.
//!
//! Args:
//! - `format`: `png`, `jpeg`, `tiff`, `bmp`, `gif` or `webp` (lossless only). Defaults to
//!   the output file's extension (useful with `output_ext = "same"`).
//! - `quality` (JPEG only, 1–100, default 85).
//! - `scale` (0.01–8.0): resize by this factor (below 1 shrinks, above 1 enlarges).
//! - `max_size` (1–65535): shrink so the longest side is at most this many pixels
//!   (never enlarges).
//!
//! EXIF orientation is applied, so phone photos come out upright.

use std::fs::File;
use std::io::BufWriter;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Rgba, RgbaImage};

use super::{arg_float, arg_int, arg_str, io_err};
use crate::pipeline::StepContext;

/// Largest result an enlargement may produce (keeps memory use reasonable).
const MAX_PIXELS: u64 = 200_000_000;

pub(super) fn run(args: &toml::Table, ctx: &StepContext) -> Result<(), String> {
    let from_ext = ctx
        .output
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let format = match arg_str(args, "format")? {
        Some(f) => f.to_owned(),
        None => match from_ext.as_deref() {
            Some("jpg" | "jpeg") => "jpeg".to_owned(),
            Some("tif" | "tiff") => "tiff".to_owned(),
            Some(e @ ("png" | "bmp" | "gif" | "webp")) => e.to_owned(),
            _ => return Err("argument `format` is required for this output".to_owned()),
        },
    };
    let format = format.as_str();
    let quality = arg_int(args, "quality", 1..=100)?.unwrap_or(85) as u8;
    let scale = arg_float(args, "scale", 0.01..=8.0)?;
    let max_size = arg_int(args, "max_size", 1..=65535)?.map(|n| n as u32);

    let mut img = load(ctx)?;
    if let Some(s) = scale {
        let w = ((img.width() as f64 * s).round() as u32).max(1);
        let h = ((img.height() as f64 * s).round() as u32).max(1);
        if u64::from(w) * u64::from(h) > MAX_PIXELS {
            return Err(format!(
                "the result would be {w}×{h} pixels, more than {} megapixels",
                MAX_PIXELS / 1_000_000
            ));
        }
        img = img.resize_exact(w, h, FilterType::Lanczos3);
    }
    if let Some(max) = max_size
        && img.width().max(img.height()) > max
    {
        img = img.resize(max, max, FilterType::Lanczos3);
    }
    save(&img, format, quality, ctx)
}

fn load(ctx: &StepContext) -> Result<DynamicImage, String> {
    let reader = ImageReader::open(&ctx.input)
        .map_err(|e| io_err("cannot open the image", e))?
        .with_guessed_format()
        .map_err(|e| io_err("cannot read the image", e))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| io_err("unsupported image", e))?;
    let orientation = decoder.orientation().ok();
    let mut img =
        DynamicImage::from_decoder(decoder).map_err(|e| io_err("cannot decode the image", e))?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(img)
}

fn save(img: &DynamicImage, format: &str, quality: u8, ctx: &StepContext) -> Result<(), String> {
    let write_err = |e: image::ImageError| io_err("cannot write the image", e);
    match format {
        "jpeg" | "jpg" => {
            let file = File::create(&ctx.output).map_err(|e| io_err("cannot create output", e))?;
            let rgb = flatten_on_white(img);
            JpegEncoder::new_with_quality(BufWriter::new(file), quality)
                .encode_image(&rgb)
                .map_err(write_err)
        }
        "png" => img
            .save_with_format(&ctx.output, ImageFormat::Png)
            .map_err(write_err),
        "tiff" => img
            .save_with_format(&ctx.output, ImageFormat::Tiff)
            .map_err(write_err),
        "bmp" => DynamicImage::ImageRgba8(img.to_rgba8())
            .save_with_format(&ctx.output, ImageFormat::Bmp)
            .map_err(write_err),
        // GIF and (lossless) WebP encoders take 8-bit RGBA.
        "gif" => DynamicImage::ImageRgba8(img.to_rgba8())
            .save_with_format(&ctx.output, ImageFormat::Gif)
            .map_err(write_err),
        "webp" => DynamicImage::ImageRgba8(img.to_rgba8())
            .save_with_format(&ctx.output, ImageFormat::WebP)
            .map_err(write_err),
        other => Err(format!(
            "unsupported format {other:?} (expected png, jpeg, tiff, bmp, gif or webp)"
        )),
    }
}

/// JPEG has no alpha channel: composite transparent pixels over white.
fn flatten_on_white(img: &DynamicImage) -> DynamicImage {
    if !img.color().has_alpha() {
        return DynamicImage::ImageRgb8(img.to_rgb8());
    }
    let rgba: RgbaImage = img.to_rgba8();
    let mut out = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, Rgba([r, g, b, a])) in rgba.enumerate_pixels().map(|(x, y, p)| (x, y, *p)) {
        let a = a as u32;
        let mix = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        out.put_pixel(x, y, image::Rgb([mix(r), mix(g), mix(b)]));
    }
    DynamicImage::ImageRgb8(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::test_util::{ctx, table};

    fn sample(dir: &std::path::Path) -> std::path::PathBuf {
        let mut img = RgbaImage::new(40, 20);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 20 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 0])
            };
        }
        let path = dir.join("sample.png");
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn converts_to_every_format() {
        let dir = tempfile::tempdir().unwrap();
        let input = sample(dir.path());
        for (format, ext) in [
            ("png", "png"),
            ("jpeg", "jpg"),
            ("tiff", "tiff"),
            ("bmp", "bmp"),
            ("gif", "gif"),
            ("webp", "webp"),
        ] {
            let out = dir.path().join(format!("out.{ext}"));
            run(&table(&format!("format = {format:?}")), &ctx(&input, &out))
                .unwrap_or_else(|e| panic!("{format}: {e}"));
            let back = image::open(&out).unwrap_or_else(|e| panic!("{format}: {e}"));
            assert_eq!((back.width(), back.height()), (40, 20), "{format}");
        }
    }

    #[test]
    fn jpeg_flattens_transparency_on_white() {
        let dir = tempfile::tempdir().unwrap();
        let input = sample(dir.path());
        let out = dir.path().join("out.jpg");
        run(
            &table("format = \"jpeg\"\nquality = 95"),
            &ctx(&input, &out),
        )
        .unwrap();
        let px = image::open(&out).unwrap().to_rgb8().get_pixel(35, 10).0;
        assert!(
            px.iter().all(|c| *c > 240),
            "transparent area should be white: {px:?}"
        );
    }

    #[test]
    fn resizes() {
        let dir = tempfile::tempdir().unwrap();
        let input = sample(dir.path());
        let out = dir.path().join("half.png");
        run(&table("format = \"png\"\nscale = 0.5"), &ctx(&input, &out)).unwrap();
        assert_eq!(image::open(&out).unwrap().width(), 20);

        let out = dir.path().join("double.png");
        run(&table("format = \"png\"\nscale = 2.0"), &ctx(&input, &out)).unwrap();
        assert_eq!(image::open(&out).unwrap().width(), 80);
        let out = dir.path().join("huge.png");
        assert!(run(&table("format = \"png\"\nscale = 9.0"), &ctx(&input, &out)).is_err());

        let out = dir.path().join("max.png");
        run(
            &table("format = \"png\"\nmax_size = 10"),
            &ctx(&input, &out),
        )
        .unwrap();
        let img = image::open(&out).unwrap();
        assert_eq!((img.width(), img.height()), (10, 5));

        // Never enlarges.
        let out = dir.path().join("big.png");
        run(
            &table("format = \"png\"\nmax_size = 1920"),
            &ctx(&input, &out),
        )
        .unwrap();
        assert_eq!(image::open(&out).unwrap().width(), 40);
    }

    #[test]
    fn errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = sample(dir.path());
        let out = dir.path().join("x.png");
        // Format inferred from the output extension.
        run(&table(""), &ctx(&input, &out)).unwrap();
        assert!(image::open(&out).is_ok());
        let odd = dir.path().join("x.data");
        assert!(
            run(&table(""), &ctx(&input, &odd))
                .unwrap_err()
                .contains("format")
        );
        assert!(
            run(&table("format = \"heic\""), &ctx(&input, &out))
                .unwrap_err()
                .contains("unsupported")
        );
        assert!(run(&table("format = \"png\"\nquality = 0"), &ctx(&input, &out)).is_err());
        let not_image = dir.path().join("a.txt");
        std::fs::write(&not_image, "hello").unwrap();
        assert!(run(&table("format = \"png\""), &ctx(&not_image, &out)).is_err());
    }
}
