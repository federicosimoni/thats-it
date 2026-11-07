//! Build tasks for *That's it*.
//!
//! ```text
//! cargo xtask icon                 render resources/AppIcon.svg → target/bundle/AppIcon.icns
//! cargo xtask bundle [--native]    build the release app → target/bundle/That's it.app
//!                                  (universal arm64 + x86_64 unless --native)
//! cargo xtask dmg [--native]       bundle, then → target/bundle/Thats-it-<version>.dmg
//!                                  (signed and, if configured, notarized + stapled)
//! ```
//!
//! **Signing.** By default the app is signed **ad hoc** (`codesign -s -`): free, required
//! for arm64, but not a Developer ID signature, so Gatekeeper asks for confirmation on first
//! launch. With a Developer ID certificate in the keychain, set
//!
//! - `THATSIT_SIGN_IDENTITY` — e.g. `Developer ID Application: Name (TEAMID)`: the app and
//!   the dmg are signed with it, with the hardened runtime and a secure timestamp;
//! - and, to notarize the dmg with `notarytool` and staple the ticket, either
//!   `THATSIT_NOTARY_PROFILE` (a keychain profile from `xcrun notarytool store-credentials`)
//!   or `THATSIT_NOTARY_KEY` + `THATSIT_NOTARY_KEY_ID` + `THATSIT_NOTARY_ISSUER`
//!   (an App Store Connect API key: path to the `.p8`, its id and the issuer id).
//!
//! `icon` works on any OS up to the PNGs; `iconutil`, `lipo`, `codesign` and `hdiutil`
//! need macOS.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use resvg::{tiny_skia, usvg};

const APP_NAME: &str = "That's it";
const BINARY: &str = "thatsit";
const TARGETS: [&str; 2] = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
const VERSION: &str = env!("CARGO_PKG_VERSION");

type Result<T = ()> = std::result::Result<T, String>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let native = args.iter().any(|a| a == "--native");
    let result = match args.first().map(String::as_str) {
        Some("icon") => icon().map(|p| println!("{}", p.display())),
        Some("bundle") => bundle(native).map(|p| println!("{}", p.display())),
        Some("dmg") => bundle(native)
            .and_then(|app| dmg(&app))
            .map(|p| println!("{}", p.display())),
        _ => Err("usage: cargo xtask <icon | bundle [--native] | dmg [--native]>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace")
        .to_path_buf()
}

fn out_dir() -> PathBuf {
    root().join("target/bundle")
}

fn run(cmd: &mut Command) -> Result {
    let shown = format!("{cmd:?}");
    eprintln!("$ {shown}");
    let status = cmd
        .status()
        .map_err(|e| format!("cannot run {shown}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{shown} failed ({status})"))
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let dest = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &dest)?;
        } else {
            fs::copy(&path, &dest).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Icon

/// Renders the SVG icon to an `.iconset` and, on macOS, to `AppIcon.icns`.
fn icon() -> Result<PathBuf> {
    let svg = fs::read(root().join("resources/AppIcon.svg")).map_err(|e| e.to_string())?;
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let iconset = out_dir().join("AppIcon.iconset");
    let _ = fs::remove_dir_all(&iconset);
    fs::create_dir_all(&iconset).map_err(|e| e.to_string())?;

    for size in [16u32, 32, 128, 256, 512] {
        for (scale, suffix) in [(1u32, ""), (2, "@2x")] {
            let px = size * scale;
            let mut pixmap = tiny_skia::Pixmap::new(px, px).ok_or("cannot allocate icon")?;
            let s = px as f32 / tree.size().width();
            resvg::render(
                &tree,
                tiny_skia::Transform::from_scale(s, s),
                &mut pixmap.as_mut(),
            );
            let path = iconset.join(format!("icon_{size}x{size}{suffix}.png"));
            pixmap.save_png(&path).map_err(|e| e.to_string())?;
        }
    }
    let icns = out_dir().join("AppIcon.icns");
    if cfg!(target_os = "macos") {
        run(Command::new("iconutil")
            .args(["-c", "icns", "-o"])
            .arg(&icns)
            .arg(&iconset))?;
        Ok(icns)
    } else {
        eprintln!("not on macOS: rendered PNGs only (iconutil is macOS-only)");
        Ok(iconset)
    }
}

// ---------------------------------------------------------------------------------------
// Bundle

fn bundle(native: bool) -> Result<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Err("bundling needs macOS (lipo, iconutil, codesign)".into());
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let targets: Vec<&str> = if native {
        vec![if cfg!(target_arch = "aarch64") {
            TARGETS[0]
        } else {
            TARGETS[1]
        }]
    } else {
        TARGETS.to_vec()
    };

    // 1. Release builds.
    for target in &targets {
        run(Command::new(&cargo).current_dir(root()).args([
            "build",
            "--release",
            "--package",
            "thatsit-app",
            "--target",
            target,
        ]))
        .map_err(|e| format!("{e}\nhint: install the target with `rustup target add {target}`"))?;
    }

    // 2. Universal binary.
    let out = out_dir();
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let binary = out.join(BINARY);
    let built: Vec<PathBuf> = targets
        .iter()
        .map(|t| root().join("target").join(t).join("release").join(BINARY))
        .collect();
    if built.len() == 1 {
        fs::copy(&built[0], &binary).map_err(|e| e.to_string())?;
    } else {
        run(Command::new("lipo")
            .arg("-create")
            .arg("-output")
            .arg(&binary)
            .args(&built))?;
    }
    run(Command::new("lipo").arg("-info").arg(&binary))?;

    // 3. Bundle layout.
    let app = out.join(format!("{APP_NAME}.app"));
    let _ = fs::remove_dir_all(&app);
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    fs::create_dir_all(&macos).map_err(|e| e.to_string())?;
    fs::create_dir_all(&resources).map_err(|e| e.to_string())?;
    fs::copy(&binary, macos.join(BINARY)).map_err(|e| e.to_string())?;

    let plist = fs::read_to_string(root().join("resources/Info.plist"))
        .map_err(|e| e.to_string())?
        .replace("@VERSION@", VERSION);
    fs::write(contents.join("Info.plist"), plist).map_err(|e| e.to_string())?;
    fs::write(contents.join("PkgInfo"), "APPL????").map_err(|e| e.to_string())?;
    run(Command::new("plutil")
        .arg("-lint")
        .arg(contents.join("Info.plist")))?;

    let icns = icon()?;
    fs::copy(&icns, resources.join("AppIcon.icns")).map_err(|e| e.to_string())?;
    copy_dir(&root().join("conversions"), &resources.join("conversions"))?;
    copy_dir(&root().join("locales"), &resources.join("locales"))?;
    for entry in fs::read_dir(root().join("resources")).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "lproj") {
            copy_dir(&path, &resources.join(path.file_name().unwrap()))?;
        }
    }
    for license in ["LICENSE", "LICENSE-MIT", "LICENSE-APACHE"] {
        let p = root().join(license);
        if p.is_file() {
            fs::copy(&p, resources.join(license)).map_err(|e| e.to_string())?;
        }
    }

    // 4. Sign (Developer ID if configured, else ad hoc), then verify.
    match sign_identity() {
        Some(identity) => run(Command::new("codesign")
            .args(["--force", "--options", "runtime", "--timestamp", "--sign"])
            .arg(&identity)
            .arg(&app))?,
        None => run(Command::new("codesign")
            .args(["--force", "--deep", "--sign", "-", "--timestamp=none"])
            .arg(&app))?,
    }
    run(Command::new("codesign")
        .args(["--verify", "--deep", "--strict", "--verbose=2"])
        .arg(&app))?;
    Ok(app)
}

// ---------------------------------------------------------------------------------------
// Signing and notarization

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The Developer ID identity from `THATSIT_SIGN_IDENTITY`, if set.
fn sign_identity() -> Option<String> {
    env_var("THATSIT_SIGN_IDENTITY")
}

/// `notarytool` credentials from the environment, if configured.
fn notary_credentials() -> Result<Option<Vec<String>>> {
    if let Some(profile) = env_var("THATSIT_NOTARY_PROFILE") {
        return Ok(Some(vec!["--keychain-profile".into(), profile]));
    }
    let key = [
        "THATSIT_NOTARY_KEY",
        "THATSIT_NOTARY_KEY_ID",
        "THATSIT_NOTARY_ISSUER",
    ]
    .map(env_var);
    match key {
        [Some(key), Some(id), Some(issuer)] => Ok(Some(vec![
            "--key".into(),
            key,
            "--key-id".into(),
            id,
            "--issuer".into(),
            issuer,
        ])),
        [None, None, None] => Ok(None),
        _ => Err("set all of THATSIT_NOTARY_KEY, THATSIT_NOTARY_KEY_ID and \
                  THATSIT_NOTARY_ISSUER, or none"
            .into()),
    }
}

/// Signs the dmg with the Developer ID and, if credentials are configured, notarizes and
/// staples it. Does nothing for ad-hoc builds.
fn sign_and_notarize_dmg(dmg: &Path) -> Result {
    let Some(identity) = sign_identity() else {
        return Ok(());
    };
    run(Command::new("codesign")
        .args(["--force", "--timestamp", "--sign"])
        .arg(&identity)
        .arg(dmg))?;
    let Some(credentials) = notary_credentials()? else {
        println!("notarization not configured: the dmg is signed but not notarized");
        return Ok(());
    };
    run(Command::new("xcrun")
        .args(["notarytool", "submit"])
        .arg(dmg)
        .args(&credentials)
        .arg("--wait"))?;
    run(Command::new("xcrun").args(["stapler", "staple"]).arg(dmg))?;
    run(Command::new("xcrun").args(["stapler", "validate"]).arg(dmg))?;
    run(Command::new("spctl")
        .args([
            "--assess",
            "--type",
            "open",
            "--context",
            "context:primary-signature",
            "-vv",
        ])
        .arg(dmg))
}

// ---------------------------------------------------------------------------------------
// DMG

fn dmg(app: &Path) -> Result<PathBuf> {
    let out = out_dir();
    let staging = out.join("dmg");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    run(Command::new("ditto")
        .arg(app)
        .arg(staging.join(format!("{APP_NAME}.app"))))?;
    std::os::unix::fs::symlink("/Applications", staging.join("Applications"))
        .map_err(|e| e.to_string())?;

    let dmg = out.join(format!("Thats-it-{VERSION}.dmg"));
    let _ = fs::remove_file(&dmg);
    run(Command::new("hdiutil")
        .args([
            "create",
            "-volname",
            APP_NAME,
            "-fs",
            "HFS+",
            "-format",
            "UDZO",
            "-ov",
            "-srcfolder",
        ])
        .arg(&staging)
        .arg(&dmg))?;
    let _ = fs::remove_dir_all(&staging);
    sign_and_notarize_dmg(&dmg)?;
    Ok(dmg)
}
