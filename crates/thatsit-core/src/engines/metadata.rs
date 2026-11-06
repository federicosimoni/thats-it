//! `builtin:strip-metadata` — removes EXIF/GPS, XMP, IPTC and comments from JPEG, PNG and
//! WebP files **without re-encoding** the pixels. Color profiles (ICC) are kept.

use std::fs;

use img_parts::jpeg::markers;
use img_parts::{Bytes, DynImage, ImageEXIF};

use super::io_err;
use crate::pipeline::StepContext;

pub(super) fn run(ctx: &StepContext) -> Result<(), String> {
    let data = fs::read(&ctx.input).map_err(|e| io_err("cannot read the image", e))?;
    let image = DynImage::from_bytes(Bytes::from(data))
        .map_err(|e| io_err("cannot parse the image", e))?
        .ok_or("only JPEG, PNG and WebP images are supported")?;
    let stripped = strip(image);
    let file = fs::File::create(&ctx.output).map_err(|e| io_err("cannot create output", e))?;
    stripped
        .encoder()
        .write_to(std::io::BufWriter::new(file))
        .map_err(|e| io_err("cannot write the image", e))?;
    Ok(())
}

fn strip(image: DynImage) -> DynImage {
    match image {
        DynImage::Jpeg(mut jpeg) => {
            // APP1 = EXIF and XMP, APP13 = IPTC/Photoshop, COM = comments.
            for marker in [markers::APP1, markers::APP13, markers::COM] {
                jpeg.remove_segments_by_marker(marker);
            }
            DynImage::Jpeg(jpeg)
        }
        DynImage::Png(mut png) => {
            for kind in [*b"eXIf", *b"tEXt", *b"zTXt", *b"iTXt", *b"tIME"] {
                png.remove_chunks_by_type(kind);
            }
            DynImage::Png(png)
        }
        DynImage::WebP(mut webp) => {
            webp.set_exif(None);
            webp.remove_chunks_by_id(*b"XMP ");
            DynImage::WebP(webp)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::test_util::ctx;
    use img_parts::jpeg::Jpeg;

    #[test]
    fn removes_exif_from_jpeg_and_keeps_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain.jpg");
        image::RgbImage::from_pixel(8, 8, image::Rgb([10, 200, 30]))
            .save(&plain)
            .unwrap();

        // Add an EXIF segment.
        let mut jpeg = Jpeg::from_bytes(Bytes::from(fs::read(&plain).unwrap())).unwrap();
        jpeg.set_exif(Some(Bytes::from_static(b"Exif\0\0GPS-secret")));
        let tagged = dir.path().join("tagged.jpg");
        jpeg.encoder()
            .write_to(fs::File::create(&tagged).unwrap())
            .unwrap();
        assert!(
            fs::read(&tagged)
                .unwrap()
                .windows(10)
                .any(|w| w == b"GPS-secret")
        );

        let out = dir.path().join("clean.jpg");
        run(&ctx(&tagged, &out)).unwrap();
        let bytes = fs::read(&out).unwrap();
        assert!(!bytes.windows(10).any(|w| w == b"GPS-secret"));
        let clean = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
        assert!(clean.exif().is_none());
        // Same image data as the original (no re-encode).
        assert_eq!(
            image::open(&out).unwrap().to_rgb8(),
            image::open(&plain).unwrap().to_rgb8()
        );
    }

    #[test]
    fn removes_text_chunks_from_png() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.png");
        image::RgbaImage::new(4, 4).save(&input).unwrap();
        let mut png =
            img_parts::png::Png::from_bytes(Bytes::from(fs::read(&input).unwrap())).unwrap();
        let chunk = img_parts::png::PngChunk::new(*b"tEXt", Bytes::from_static(b"Author\0me"));
        let at = png.chunks().len() - 1;
        png.chunks_mut().insert(at, chunk);
        let tagged = dir.path().join("tagged.png");
        png.encoder()
            .write_to(fs::File::create(&tagged).unwrap())
            .unwrap();

        let out = dir.path().join("clean.png");
        run(&ctx(&tagged, &out)).unwrap();
        let clean = img_parts::png::Png::from_bytes(Bytes::from(fs::read(&out).unwrap())).unwrap();
        assert!(clean.chunk_by_type(*b"tEXt").is_none());
        image::open(&out).unwrap();
    }

    #[test]
    fn rejects_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.txt");
        fs::write(&input, "hello").unwrap();
        assert!(run(&ctx(&input, &dir.path().join("o"))).is_err());
    }
}
