//! Reading dragged file URLs from a pasteboard and resolving their UTTypes.

use std::path::Path;

use objc2::ClassType;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_app_kit::{NSPasteboard, NSPasteboardURLReadingFileURLsOnlyKey};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSURL, NSURLContentTypeKey};
use objc2_uniform_type_identifiers::UTType;
use thatsit_core::conversion::FileInfo;

/// A dragged file: its path and its resolved UTType identifiers.
#[derive(Debug, Clone)]
pub struct DraggedFile {
    pub path: String,
    /// The file's own UTType identifier, if it could be resolved.
    pub uti: Option<String>,
    /// The UTType followed by every type it conforms to (for matching).
    pub types: Vec<String>,
}

impl DraggedFile {
    pub fn file_info(&self) -> FileInfo {
        FileInfo::new(Path::new(&self.path), self.types.clone())
    }
}

/// Reads the file URLs on `pasteboard` (non-file URLs are ignored).
pub fn read_files(pasteboard: &NSPasteboard) -> Vec<DraggedFile> {
    let classes: Retained<NSArray<AnyClass>> = NSArray::from_slice(&[NSURL::class()]);
    let yes = NSNumber::new_bool(true);
    // SAFETY: `NSPasteboardURLReadingFileURLsOnlyKey` is an immutable NSString constant
    // exported by AppKit; reading an extern static is sound once AppKit is loaded.
    let key = unsafe { NSPasteboardURLReadingFileURLsOnlyKey };
    let value: &AnyObject = &yes;
    let options = NSDictionary::from_slices(&[key], &[value]);

    // SAFETY: the class array contains only `NSURL`, which conforms to
    // `NSPasteboardReading`, and the options dictionary uses a documented key with an
    // `NSNumber` (boolean) value, as the API expects.
    let objects = unsafe { pasteboard.readObjectsForClasses_options(&classes, Some(&options)) };
    let Some(objects) = objects else {
        return Vec::new();
    };

    objects
        .iter()
        .filter_map(|obj| obj.downcast::<NSURL>().ok())
        .filter_map(|url| {
            let path = url.path()?.to_string();
            let uttype = content_type(&url);
            Some(DraggedFile {
                uti: uttype.as_ref().map(|t| t.identifier().to_string()),
                types: uttype.as_deref().map(conformance).unwrap_or_default(),
                path,
            })
        })
        .collect()
}

/// A synthetic file for the visual self-test (`THATSIT_DEMO`), typed by extension.
pub fn demo_file(path: &str) -> DraggedFile {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let uttype = UTType::typeWithFilenameExtension(&NSString::from_str(ext));
    DraggedFile {
        path: path.to_owned(),
        uti: uttype.as_ref().map(|t| t.identifier().to_string()),
        types: uttype.as_deref().map(conformance).unwrap_or_default(),
    }
}

/// The identifier of `uttype` followed by all its supertypes.
fn conformance(uttype: &UTType) -> Vec<String> {
    let mut types = vec![uttype.identifier().to_string()];
    let mut supers: Vec<String> = uttype
        .supertypes()
        .iter()
        .map(|t| t.identifier().to_string())
        .collect();
    supers.sort();
    types.extend(supers);
    types
}

/// Resolves the UTType of a file URL via `NSURLContentTypeKey`.
fn content_type(url: &NSURL) -> Option<Retained<UTType>> {
    // SAFETY: `NSURLContentTypeKey` is an immutable NSString constant exported by Foundation.
    let key: &NSString = unsafe { NSURLContentTypeKey };
    let values = url
        .resourceValuesForKeys_error(&NSArray::from_slice(&[key]))
        .ok()?;
    let value = values.objectForKey(key)?;
    value.downcast::<UTType>().ok()
}
