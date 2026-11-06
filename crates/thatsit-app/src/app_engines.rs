//! Builtin engines that need AppKit or PDFKit.
//!
//! - `builtin:doc-to-pdf`: loads a document into an `NSAttributedString` (RTF, DOC, DOCX,
//!   ODT, HTML, TXT…) and prints it to a paginated PDF with the default paper size.
//!   Runs on the main thread (AppKit text and printing are main-thread only).
//! - `builtin:pdf-to-images`: renders every PDF page to a PNG with PDFKit.
//!   Args: `dpi` (36–600, default 144). Output: a directory, `<stem>-001.png`…

use std::path::Path;

use dispatch2::DispatchQueue;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua,
    NSAttributedStringDocumentFormats, NSBitmapImageFileType, NSBitmapImageRep, NSPrintInfo,
    NSPrintJobSavingURL, NSPrintOperation, NSPrintSaveJob, NSPrintingPaginationMode, NSTextView,
};
use objc2_foundation::{
    NSAttributedString, NSDictionary, NSPoint, NSRect, NSSize, NSString, NSURL,
};
use objc2_pdf_kit::{PDFDisplayBox, PDFDocument};
use thatsit_core::pipeline::{CancelToken, EngineRunner, StepContext};
use thatsit_core::toml;

/// Names of the engines implemented here.
pub const APP_ENGINES: &[&str] = &["builtin:doc-to-pdf", "builtin:pdf-to-images"];

/// Page margins for documents printed to PDF, in points (¾ inch).
const MARGIN: f64 = 54.0;

pub struct AppEngines;

impl EngineRunner for AppEngines {
    fn run(
        &self,
        name: &str,
        args: &toml::Table,
        ctx: &StepContext,
        cancel: &CancelToken,
    ) -> Result<(), String> {
        match name {
            "builtin:doc-to-pdf" => on_main(|mtm| doc_to_pdf(&ctx.input, &ctx.output, mtm)),
            "builtin:pdf-to-images" => {
                let dpi = match args.get("dpi") {
                    None => 144.0,
                    Some(toml::Value::Integer(n)) if (36..=600).contains(n) => *n as f64,
                    Some(_) => return Err("argument `dpi` must be an integer in 36..=600".into()),
                };
                pdf_to_images(&ctx.input, &ctx.output, &ctx.stem, dpi, cancel)
            }
            other => Err(format!("unknown engine {other}")),
        }
    }
}

/// Runs `f` on the main thread and waits for its result.
fn on_main<T: Send>(f: impl FnOnce(MainThreadMarker) -> T + Send) -> T {
    if let Some(mtm) = MainThreadMarker::new() {
        return f(mtm);
    }
    let mut result = None;
    DispatchQueue::main().exec_sync(|| {
        let mtm = MainThreadMarker::new().expect("dispatched to the main queue");
        result = Some(f(mtm));
    });
    result.expect("main-queue work ran")
}

fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

fn doc_to_pdf(input: &Path, output: &Path, mtm: MainThreadMarker) -> Result<(), String> {
    autoreleasepool(|_| {
        let options = NSDictionary::<NSString, AnyObject>::new();
        // SAFETY: an empty options dictionary is valid (AppKit detects the format), and we
        // do not request the document attributes.
        let text = unsafe {
            NSAttributedString::initWithURL_options_documentAttributes_error(
                NSAttributedString::alloc(),
                &file_url(input),
                &options,
                None,
            )
        }
        .map_err(|e| format!("cannot read the document: {}", e.localizedDescription()))?;

        // A private copy of the default print settings (paper size from the user's locale).
        // SAFETY: `dictionary` returns the shared info's attributes; we only read them to
        // build an independent NSPrintInfo.
        let info = unsafe {
            NSPrintInfo::initWithDictionary(
                NSPrintInfo::alloc(),
                &NSPrintInfo::sharedPrintInfo().dictionary(),
            )
        };
        for set in [
            NSPrintInfo::setTopMargin,
            NSPrintInfo::setBottomMargin,
            NSPrintInfo::setLeftMargin,
            NSPrintInfo::setRightMargin,
        ] {
            set(&info, MARGIN);
        }
        info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
        info.setVerticalPagination(NSPrintingPaginationMode::Automatic);
        // SAFETY: `NSPrintSaveJob` / `NSPrintJobSavingURL` are immutable AppKit constants;
        // the saving-URL attribute takes an NSURL, as documented.
        unsafe {
            info.setJobDisposition(NSPrintSaveJob);
            info.dictionary()
                .insert(NSPrintJobSavingURL, &*file_url(output) as &AnyObject);
        }

        let width = info.paperSize().width - 2.0 * MARGIN;
        let view = NSTextView::initWithFrame(
            NSTextView::alloc(mtm),
            NSRect::new(NSPoint::ZERO, NSSize::new(width, 100.0)),
        );
        // Print with light-mode colors whatever the system appearance.
        // SAFETY: `NSAppearanceNameAqua` is an immutable AppKit constant.
        view.setAppearance(
            NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }).as_deref(),
        );
        view.setHorizontallyResizable(false);
        view.setVerticallyResizable(true);
        view.setMaxSize(NSSize::new(width, 1.0e7));
        // SAFETY: a freshly created NSTextView always has a text storage and container.
        unsafe {
            if let Some(container) = view.textContainer() {
                container.setWidthTracksTextView(true);
            }
            if let Some(storage) = view.textStorage() {
                storage.setAttributedString(&text);
            }
        }
        view.sizeToFit();

        let op = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
        op.setShowsPrintPanel(false);
        op.setShowsProgressPanel(false);
        if !op.runOperation() {
            return Err("printing to PDF failed".into());
        }
        if !output.is_file() {
            return Err("no PDF was produced".into());
        }
        Ok(())
    })
}

fn pdf_to_images(
    input: &Path,
    output_dir: &Path,
    stem: &str,
    dpi: f64,
    cancel: &CancelToken,
) -> Result<(), String> {
    // SAFETY: `initWithURL:` takes a file URL and returns nil for unreadable PDFs.
    let doc = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &file_url(input)) }
        .ok_or("cannot open the PDF (damaged or password-protected?)")?;
    // SAFETY: plain getter on a valid document.
    let count = unsafe { doc.pageCount() };
    if count == 0 {
        return Err("the PDF has no pages".into());
    }
    let scale = dpi / 72.0;
    let digits = count.to_string().len().max(3);
    for i in 0..count {
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        autoreleasepool(|_| -> Result<(), String> {
            // SAFETY: `i < pageCount`, so the page exists.
            let page = unsafe { doc.pageAtIndex(i) }.ok_or("cannot read a page")?;
            // SAFETY: plain getters on a valid page.
            let bounds = unsafe { page.boundsForBox(PDFDisplayBox::MediaBox) };
            let size = NSSize::new(
                (bounds.size.width * scale).round(),
                (bounds.size.height * scale).round(),
            );
            // SAFETY: `thumbnailOfSize:forBox:` renders the page into a new NSImage.
            let image = unsafe { page.thumbnailOfSize_forBox(size, PDFDisplayBox::MediaBox) };
            let tiff = image.TIFFRepresentation().ok_or("cannot render a page")?;
            let rep = NSBitmapImageRep::imageRepWithData(&tiff).ok_or("cannot render a page")?;
            let props = NSDictionary::new();
            // SAFETY: PNG with no extra properties is a valid request.
            let png = unsafe {
                rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props)
            }
            .ok_or("cannot encode a page as PNG")?;
            let path = output_dir.join(format!("{stem}-{:0digits$}.png", i + 1));
            if !png.writeToFile_atomically(&NSString::from_str(&path.to_string_lossy()), true) {
                return Err(format!("cannot write {}", path.display()));
            }
            Ok(())
        })?;
    }
    Ok(())
}
