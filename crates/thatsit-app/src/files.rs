//! Reading dragged file URLs from a pasteboard and resolving their UTTypes.

use objc2::ClassType;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_app_kit::{NSPasteboard, NSPasteboardURLReadingFileURLsOnlyKey};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSURL, NSURLContentTypeKey};
use objc2_uniform_type_identifiers::UTType;

/// A dragged file: its path and its resolved UTType identifier (if any).
#[derive(Debug, Clone)]
pub struct DraggedFile {
    pub path: String,
    pub uti: Option<String>,
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
            Some(DraggedFile {
                uti: content_type(&url),
                path,
            })
        })
        .collect()
}

/// Resolves the UTType identifier of a file URL via `NSURLContentTypeKey`.
fn content_type(url: &NSURL) -> Option<String> {
    // SAFETY: `NSURLContentTypeKey` is an immutable NSString constant exported by Foundation.
    let key: &NSString = unsafe { NSURLContentTypeKey };
    let values = url
        .resourceValuesForKeys_error(&NSArray::from_slice(&[key]))
        .ok()?;
    let value = values.objectForKey(key)?;
    let uttype = value.downcast::<UTType>().ok()?;
    Some(uttype.identifier().to_string())
}
