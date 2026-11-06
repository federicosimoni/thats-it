//! Runs every bundled conversion that the core can run (command steps + pure-Rust engines)
//! on freshly generated sample files, with the real macOS tools.
//!
//! macOS only. Conversions whose tools are missing (e.g. `ffmpeg`) are skipped and listed;
//! conversions that need the app's AppKit/PDFKit engines are covered by the app.

#![cfg(target_os = "macos")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use thatsit_core::conversion::{Availability, FileInfo, Origin, Output, Registry};
use thatsit_core::engines::{CORE_ENGINES, CoreEngines};
use thatsit_core::output::PendingOutput;
use thatsit_core::pipeline::{self, CancelToken};
use thatsit_core::tools::ToolFinder;

/// A sample file and the UTTypes it conforms to (the app gets these from macOS).
struct Sample {
    path: PathBuf,
    types: Vec<&'static str>,
}

fn sh(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn make_samples(dir: &Path, tools: &ToolFinder) -> Vec<Sample> {
    let mut samples = Vec::new();
    let mut add = |name: &str, types: &[&'static str]| {
        let path = dir.join(name);
        if path.exists() {
            samples.push(Sample {
                path,
                types: types.to_vec(),
            });
        }
    };
    let image_types = |uti: &'static str| vec![uti, "public.image", "public.data", "public.item"];

    // Raster images.
    let mut img = image::RgbaImage::new(64, 48);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = image::Rgba([(x * 4) as u8, (y * 5) as u8, 128, 255]);
    }
    for ext in ["png", "jpg", "tiff", "bmp", "gif", "webp"] {
        let path = dir.join(format!("sample.{ext}"));
        let rgba = image::DynamicImage::ImageRgba8(img.clone());
        let result = if ext == "jpg" {
            image::DynamicImage::ImageRgb8(rgba.to_rgb8()).save(&path)
        } else {
            rgba.save(&path)
        };
        result.unwrap();
    }
    let big = dir.join("big.png");
    image::RgbaImage::from_pixel(1024, 1024, image::Rgba([30, 120, 220, 255]))
        .save(&big)
        .unwrap();
    for (name, uti) in [
        ("sample.png", "public.png"),
        ("sample.jpg", "public.jpeg"),
        ("sample.tiff", "public.tiff"),
        ("sample.bmp", "com.microsoft.bmp"),
        ("sample.gif", "com.compuserve.gif"),
        ("sample.webp", "org.webmproject.webp"),
        ("big.png", "public.png"),
    ] {
        add(name, &image_types(uti));
    }
    if sh(
        "sips",
        &[
            "-s",
            "format",
            "heic",
            dir.join("big.png").to_str().unwrap(),
            "--out",
            dir.join("sample.heic").to_str().unwrap(),
        ],
    ) {
        add("sample.heic", &image_types("public.heic"));
    }
    fs::write(
        dir.join("sample.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><circle cx="20" cy="15" r="12" fill="#09f"/></svg>"##,
    )
    .unwrap();
    add(
        "sample.svg",
        &[
            "public.svg-image",
            "public.image",
            "public.data",
            "public.item",
        ],
    );

    // Audio: a 0.5 s 440 Hz WAV, converted to AIFF/CAF/M4A with afconvert.
    write_wav(&dir.join("sample.wav"));
    add(
        "sample.wav",
        &[
            "com.microsoft.waveform-audio",
            "public.audio",
            "public.data",
            "public.item",
        ],
    );
    let wav = dir.join("sample.wav");
    for (ext, fmt, data, uti) in [
        ("aiff", "AIFF", "BEI16", "public.aiff-audio"),
        ("caf", "caff", "LEI16", "com.apple.coreaudio-format"),
        ("m4a", "m4af", "aac", "com.apple.m4a-audio"),
    ] {
        let out = dir.join(format!("sample.{ext}"));
        if sh(
            "afconvert",
            &[
                "-f",
                fmt,
                "-d",
                data,
                wav.to_str().unwrap(),
                out.to_str().unwrap(),
            ],
        ) {
            add(
                &format!("sample.{ext}"),
                &[uti, "public.audio", "public.data", "public.item"],
            );
        }
    }

    // Documents: TXT, HTML, Markdown, then RTF/DOCX/DOC/ODT via textutil.
    fs::write(
        dir.join("sample.txt"),
        "Hello from That's it.\nSecond line.\n",
    )
    .unwrap();
    add(
        "sample.txt",
        &[
            "public.plain-text",
            "public.text",
            "public.data",
            "public.item",
        ],
    );
    fs::write(
        dir.join("sample.html"),
        "<html><body><h1>Hello</h1><p>World</p></body></html>",
    )
    .unwrap();
    add(
        "sample.html",
        &["public.html", "public.text", "public.data", "public.item"],
    );
    fs::write(
        dir.join("sample.md"),
        "# Hello\n\n*World* and a [link](https://example.com).\n",
    )
    .unwrap();
    add(
        "sample.md",
        &[
            "net.daringfireball.markdown",
            "public.plain-text",
            "public.text",
            "public.data",
            "public.item",
        ],
    );
    let txt = dir.join("sample.txt");
    for (ext, uti) in [
        ("rtf", "public.rtf"),
        ("docx", "org.openxmlformats.wordprocessingml.document"),
        ("doc", "com.microsoft.word.doc"),
        ("odt", "org.oasis-open.opendocument.text"),
    ] {
        let out = dir.join(format!("sample.{ext}"));
        if sh(
            "textutil",
            &[
                "-convert",
                ext,
                "-output",
                out.to_str().unwrap(),
                txt.to_str().unwrap(),
            ],
        ) {
            add(
                &format!("sample.{ext}"),
                &[uti, "public.data", "public.item"],
            );
        }
    }

    // Video (only with ffmpeg).
    if let Some(ffmpeg) = tools.find("ffmpeg") {
        let out = dir.join("sample.mov");
        let ok = Command::new(ffmpeg)
            .args([
                "-nostdin",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=320x240:rate=12",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=1",
                "-shortest",
                "-c:v",
                "mjpeg",
                "-c:a",
                "pcm_s16le",
            ])
            .arg(&out)
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            add(
                "sample.mov",
                &[
                    "com.apple.quicktime-movie",
                    "public.movie",
                    "public.audiovisual-content",
                    "public.data",
                    "public.item",
                ],
            );
        }
    }

    // A folder (for ZIP).
    fs::create_dir_all(dir.join("folder/sub")).unwrap();
    fs::write(dir.join("folder/sub/a.txt"), "a").unwrap();
    add(
        "folder",
        &["public.folder", "public.directory", "public.item"],
    );
    samples
}

fn write_wav(path: &Path) {
    let rate = 44_100u32;
    let samples: Vec<i16> = (0..rate / 2)
        .map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 8000.0) as i16)
        .collect();
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    fs::write(path, b).unwrap();
}

#[test]
fn run_every_bundled_conversion() {
    let dir = tempfile::tempdir().unwrap();
    let tools = ToolFinder::from_env();
    let samples = make_samples(dir.path(), &tools);

    let mut registry = Registry::new();
    registry.load_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conversions"),
        Origin::Bundled,
    );
    assert!(registry.issues().is_empty(), "{:?}", registry.issues());

    let mut availability = Availability {
        engines: CORE_ENGINES.iter().map(|s| s.to_string()).collect(),
        ..Availability::default()
    };
    for c in registry.conversions() {
        for t in &c.requires {
            if tools.find(t).is_some() {
                availability.tools.insert(t.clone());
            }
        }
    }

    let mut passed = Vec::new();
    let mut skipped = Vec::new();
    let mut failed = Vec::new();
    let out_dir = dir.path().join("out");
    fs::create_dir(&out_dir).unwrap();

    for conversion in registry.conversions() {
        if !availability.can_run(conversion) {
            let mut missing = availability.missing_tools(conversion);
            missing.extend(availability.missing_engines(conversion));
            skipped.push(format!("{} (needs {})", conversion.id, missing.join(", ")));
            continue;
        }
        // Prefer the big square PNG for the icon conversion.
        let candidates: Vec<&Sample> = samples
            .iter()
            .filter(|s| {
                let info = FileInfo::new(&s.path, s.types.iter().map(|t| t.to_string()).collect());
                registry
                    .matching(&[info], &availability)
                    .iter()
                    .any(|c| c.id == conversion.id)
            })
            .collect();
        let sample = if conversion.id == "image.png-to-icns" {
            candidates
                .iter()
                .find(|s| s.path.ends_with("big.png"))
                .copied()
        } else {
            candidates.first().copied()
        };
        let Some(sample) = sample else {
            skipped.push(format!("{} (no sample file)", conversion.id));
            continue;
        };

        // Copy the sample so outputs land in a clean folder next to it.
        let input = out_dir.join(sample.path.file_name().unwrap());
        if !input.exists() {
            if sample.path.is_dir() {
                assert!(sh(
                    "cp",
                    &["-R", sample.path.to_str().unwrap(), input.to_str().unwrap()]
                ));
            } else {
                fs::copy(&sample.path, &input).unwrap();
            }
        }
        let kind = conversion.output_for(&input).unwrap();
        let pending =
            PendingOutput::prepare(&input, &kind, &conversion.output_suffix, &out_dir).unwrap();
        let result = pipeline::run(
            conversion,
            &input,
            &pending.temp_path,
            &tools,
            &CoreEngines,
            &CancelToken::new(),
        );
        match result
            .map_err(|e| e.to_string())
            .and_then(|_| pending.commit().map_err(|e| e.to_string()))
        {
            Ok(path) => {
                let size = if path.is_dir() {
                    fs::read_dir(&path).unwrap().count() as u64
                } else {
                    fs::metadata(&path).unwrap().len()
                };
                if size == 0 {
                    failed.push(format!(
                        "{}: empty output {}",
                        conversion.id,
                        path.display()
                    ));
                } else {
                    if let Output::File { ext } = &kind
                        && ["png", "jpg", "tiff", "bmp", "gif", "webp"].contains(&ext.as_str())
                        && let Err(e) = image::open(&path)
                    {
                        failed.push(format!(
                            "{}: output is not a valid image: {e}",
                            conversion.id
                        ));
                        continue;
                    }
                    passed.push(format!(
                        "{} ← {} → {}",
                        conversion.id,
                        sample.path.file_name().unwrap().to_string_lossy(),
                        path.file_name().unwrap().to_string_lossy()
                    ));
                }
            }
            Err(e) => failed.push(format!("{}: {e}", conversion.id)),
        }
    }

    println!("PASSED ({}):\n  {}", passed.len(), passed.join("\n  "));
    println!("SKIPPED ({}):\n  {}", skipped.len(), skipped.join("\n  "));
    println!("FAILED ({}):\n  {}", failed.len(), failed.join("\n  "));
    assert!(
        failed.is_empty(),
        "{} conversion(s) failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    assert!(
        passed.len() >= 20,
        "too few conversions ran: {}",
        passed.len()
    );
}
