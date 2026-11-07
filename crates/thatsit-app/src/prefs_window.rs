//! The Preferences window: General, Conversions and Tools tabs.
//!
//! Built in code with fixed frames inside flipped views (top-to-bottom layout), so it
//! needs no nib and no Auto Layout. Every change is applied immediately.

use std::cell::{Cell, OnceCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel,
};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSButton, NSControlStateValueOff, NSControlStateValueOn,
    NSFont, NSFontWeightSemibold, NSModalResponseOK, NSOpenPanel, NSPopUpButton, NSScreen,
    NSScrollView, NSTabView, NSTabViewItem, NSTextField, NSUserInterfaceItemIdentification, NSView,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSBundle, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use objc2_service_management::{SMAppService, SMAppServiceStatus};
use thatsit_core::conversion::Category;
use thatsit_core::i18n::Arg;

use crate::log;
use crate::prefs::{self, WidgetSize};
use crate::ring_view::RingView;
use crate::status::activate;

const WIDTH: f64 = 560.0;
const HEIGHT: f64 = 520.0;
const MARGIN: f64 = 24.0;
const LABEL_W: f64 = 230.0;

// A view whose y axis grows downwards, so rows can be laid out from the top.
define_class!(
    // SAFETY: NSView has no subclassing requirements beyond main-thread use; no `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    struct FlippedView;

    impl FlippedView {
        // SAFETY: the signature matches `-[NSView isFlipped]`.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl FlippedView {
    fn new(mtm: MainThreadMarker, size: NSSize) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `initWithFrame:` is NSView's designated initializer.
        unsafe { msg_send![super(this), initWithFrame: NSRect::new(NSPoint::ZERO, size)] }
    }
}

pub struct PrefsIvars {
    view: Retained<RingView>,
    window: OnceCell<Retained<NSWindow>>,
    screens: OnceCell<Vec<String>>,
    /// Index of the tab to reselect after a rebuild.
    tab: Cell<isize>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; `PrefsController` has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = PrefsIvars]
    pub struct PrefsController;

    impl PrefsController {
        // SAFETY (all actions): the signature matches `- (void)x:(id)sender`.
        #[unsafe(method(screenChanged:))]
        fn screen_changed(&self, sender: Option<&NSPopUpButton>) {
            let Some(popup) = sender else { return };
            let index = popup.indexOfSelectedItem();
            let names = self.ivars().screens.get().cloned().unwrap_or_default();
            // Index 0 is "main screen".
            let name = (index > 0).then(|| names.get(index as usize - 1).cloned()).flatten();
            prefs::set_screen(name.as_deref());
            self.ivars().view.apply_preferences();
        }

        #[unsafe(method(sizeChanged:))]
        fn size_changed(&self, sender: Option<&NSPopUpButton>) {
            let Some(popup) = sender else { return };
            if let Some(size) = WidgetSize::ALL.get(popup.indexOfSelectedItem().max(0) as usize) {
                prefs::set_widget_size(*size);
                self.ivars().view.apply_preferences();
            }
        }

        #[unsafe(method(loginToggled:))]
        fn login_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else { return };
            let on = button.state() == NSControlStateValueOn;
            // SAFETY: `mainAppService` describes this bundled app; register/unregister
            // report failures through NSError.
            let result = unsafe {
                let service = SMAppService::mainAppService();
                if on { service.registerAndReturnError() } else { service.unregisterAndReturnError() }
            };
            if let Err(e) = result {
                log::info(format!("open at login: {}", e.localizedDescription()));
            }
            button.setState(if login_enabled() { NSControlStateValueOn } else { NSControlStateValueOff });
        }

        #[unsafe(method(accentToggled:))]
        fn accent_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else { return };
            prefs::set_use_system_accent(button.state() == NSControlStateValueOn);
            self.ivars().view.apply_preferences();
        }

        #[unsafe(method(chooseFallback:))]
        fn choose_fallback(&self, _sender: Option<&AnyObject>) {
            let panel = NSOpenPanel::openPanel(self.mtm());
            panel.setCanChooseDirectories(true);
            panel.setCanChooseFiles(false);
            panel.setCanCreateDirectories(true);
            if panel.runModal() == NSModalResponseOK
                && let Some(path) = panel.URL().and_then(|u| u.path())
            {
                prefs::set_fallback_dir(&path.to_string());
                self.rebuild();
            }
        }

        #[unsafe(method(conversionToggled:))]
        fn conversion_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else { return };
            if let Some(id) = button.identifier() {
                prefs::set_conversion_enabled(&id.to_string(), button.state() == NSControlStateValueOn);
            }
        }

        #[unsafe(method(rescan:))]
        fn rescan(&self, _sender: Option<&AnyObject>) {
            self.ivars().view.catalog().rescan_tools();
            self.rebuild();
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for PrefsController {}
);

impl PrefsController {
    pub fn new(mtm: MainThreadMarker, view: Retained<RingView>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PrefsIvars {
            view,
            window: OnceCell::new(),
            screens: OnceCell::new(),
            tab: Cell::new(0),
        });
        // SAFETY: `init` is NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    /// Opens (or brings to front) the window, rescanning tools first.
    pub fn show(&self) {
        let mtm = self.mtm();
        let iv = self.ivars();
        iv.view.catalog().rescan_tools();
        let window = iv.window.get_or_init(|| {
            // SAFETY: released-when-closed is disabled right below.
            let window = unsafe {
                NSWindow::initWithContentRect_styleMask_backing_defer(
                    NSWindow::alloc(mtm),
                    NSRect::new(NSPoint::ZERO, NSSize::new(WIDTH, HEIGHT)),
                    NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                    NSBackingStoreType::Buffered,
                    false,
                )
            };
            // SAFETY: we keep the window alive in `ivars`.
            unsafe { window.setReleasedWhenClosed(false) };
            window.setTitle(&NSString::from_str(
                &iv.view.catalog().i18n.tr("prefs-title"),
            ));
            window.center();
            window
        });
        self.rebuild();
        activate(&NSApplication::sharedApplication(mtm));
        window.makeKeyAndOrderFront(None);
    }

    /// Rebuilds the window content from the current preferences.
    fn rebuild(&self) {
        let mtm = self.mtm();
        let iv = self.ivars();
        let Some(window) = iv.window.get() else {
            return;
        };
        let i18n = &iv.view.catalog().i18n;

        if let Some(old) = window
            .contentView()
            .and_then(|v| v.downcast::<NSTabView>().ok())
        {
            if let Some(selected) = old.selectedTabViewItem() {
                iv.tab.set(old.indexOfTabViewItem(&selected));
            }
        }
        let tabs = NSTabView::initWithFrame(
            NSTabView::alloc(mtm),
            NSRect::new(NSPoint::ZERO, NSSize::new(WIDTH, HEIGHT)),
        );
        for (id, content) in [
            ("prefs-tab-general", self.general_tab()),
            ("prefs-tab-conversions", self.conversions_tab()),
            ("prefs-tab-tools", self.tools_tab()),
        ] {
            // SAFETY: an NSString identifier is valid for a tab item.
            let item = unsafe {
                NSTabViewItem::initWithIdentifier(
                    NSTabViewItem::alloc(),
                    Some(&NSString::from_str(id)),
                )
            };
            item.setLabel(&NSString::from_str(&i18n.tr(id)));
            item.setView(Some(&content));
            tabs.addTabViewItem(&item);
        }
        tabs.selectTabViewItemAtIndex(iv.tab.get().max(0));
        window.setContentView(Some(&tabs));
    }

    fn content_size() -> NSSize {
        NSSize::new(WIDTH - 40.0, HEIGHT - 70.0)
    }

    fn general_tab(&self) -> Retained<NSView> {
        let mtm = self.mtm();
        let i18n = &self.ivars().view.catalog().i18n;
        let root = FlippedView::new(mtm, Self::content_size());
        let mut y = MARGIN;
        let row = |y: f64| NSRect::new(NSPoint::new(MARGIN, y), NSSize::new(LABEL_W, 22.0));
        let control = |y: f64, w: f64| {
            NSRect::new(
                NSPoint::new(MARGIN + LABEL_W, y - 2.0),
                NSSize::new(w, 26.0),
            )
        };

        // Screen.
        root.addSubview(&label(&i18n.tr("prefs-screen"), row(y), mtm));
        let screen_popup = popup(control(y, 240.0), self, sel!(screenChanged:), mtm);
        let names: Vec<String> = NSScreen::screens(mtm)
            .iter()
            .map(|s| s.localizedName().to_string())
            .collect();
        let main_name = NSScreen::mainScreen(mtm)
            .map(|s| s.localizedName().to_string())
            .unwrap_or_default();
        screen_popup.addItemWithTitle(&NSString::from_str(&format!("{main_name} ★")));
        for n in &names {
            screen_popup.addItemWithTitle(&NSString::from_str(n));
        }
        let chosen = prefs::screen();
        let index = chosen
            .and_then(|c| names.iter().position(|n| *n == c))
            .map(|i| i + 1)
            .unwrap_or(0);
        screen_popup.selectItemAtIndex(index as isize);
        let _ = self.ivars().screens.set(names);
        root.addSubview(&screen_popup);
        y += 40.0;

        // Size.
        root.addSubview(&label(&i18n.tr("prefs-size"), row(y), mtm));
        let size_popup = popup(control(y, 160.0), self, sel!(sizeChanged:), mtm);
        for size in WidgetSize::ALL {
            size_popup.addItemWithTitle(&NSString::from_str(&i18n.tr(size.message_id())));
        }
        let current = WidgetSize::ALL
            .iter()
            .position(|s| *s == prefs::widget_size())
            .unwrap_or(1);
        size_popup.selectItemAtIndex(current as isize);
        root.addSubview(&size_popup);
        y += 48.0;

        // Open at login.
        let login = checkbox(&i18n.tr("prefs-login"), self, sel!(loginToggled:), mtm);
        login.setFrame(NSRect::new(
            NSPoint::new(MARGIN, y),
            NSSize::new(WIDTH - 2.0 * MARGIN - 40.0, 22.0),
        ));
        let bundled = NSBundle::mainBundle().bundleIdentifier().is_some();
        login.setEnabled(bundled);
        login.setState(if bundled && login_enabled() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        root.addSubview(&login);
        y += 24.0;
        if !bundled {
            let note = label(
                &i18n.tr("prefs-login-unavailable"),
                NSRect::new(NSPoint::new(MARGIN + 20.0, y), NSSize::new(420.0, 18.0)),
                mtm,
            );
            note.setFont(Some(&NSFont::systemFontOfSize(11.0)));
            root.addSubview(&note);
            y += 22.0;
        }
        y += 8.0;

        // System accent.
        let accent = checkbox(&i18n.tr("prefs-accent"), self, sel!(accentToggled:), mtm);
        accent.setFrame(NSRect::new(
            NSPoint::new(MARGIN, y),
            NSSize::new(WIDTH - 2.0 * MARGIN - 40.0, 22.0),
        ));
        accent.setState(if prefs::use_system_accent() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        root.addSubview(&accent);
        y += 48.0;

        // Fallback folder.
        root.addSubview(&label(
            &i18n.tr("prefs-fallback"),
            NSRect::new(
                NSPoint::new(MARGIN, y),
                NSSize::new(WIDTH - 2.0 * MARGIN - 40.0, 22.0),
            ),
            mtm,
        ));
        y += 28.0;
        let path = prefs::fallback_dir().display().to_string();
        let path_label = label(
            &path,
            NSRect::new(NSPoint::new(MARGIN, y + 3.0), NSSize::new(340.0, 20.0)),
            mtm,
        );
        root.addSubview(&path_label);
        // SAFETY: the action is implemented by `self`, the target.
        let choose = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(&i18n.tr("prefs-choose")),
                Some(self),
                Some(sel!(chooseFallback:)),
                mtm,
            )
        };
        choose.setFrame(NSRect::new(
            NSPoint::new(MARGIN + 350.0, y - 2.0),
            NSSize::new(110.0, 28.0),
        ));
        root.addSubview(&choose);

        Retained::into_super(root)
    }

    fn conversions_tab(&self) -> Retained<NSView> {
        let mtm = self.mtm();
        let catalog = self.ivars().view.catalog();
        let i18n = &catalog.i18n;
        let availability = catalog.availability();
        let disabled = prefs::disabled_conversions();
        let size = Self::content_size();

        let rows: usize = catalog.registry.conversions().len() + Category::ALL.len() * 2 + 2;
        let doc_height = (rows as f64) * 24.0 + 2.0 * MARGIN;
        let doc = FlippedView::new(mtm, NSSize::new(size.width - 20.0, doc_height));
        let mut y = MARGIN / 2.0;
        let hint = label(
            &i18n.tr("prefs-conversions-hint"),
            NSRect::new(
                NSPoint::new(MARGIN, y),
                NSSize::new(size.width - 2.0 * MARGIN, 20.0),
            ),
            mtm,
        );
        doc.addSubview(&hint);
        y += 32.0;

        for category in Category::ALL {
            let items: Vec<_> = catalog
                .registry
                .conversions()
                .iter()
                .filter(|c| c.category == category)
                .collect();
            if items.is_empty() {
                continue;
            }
            let header = label(
                &catalog.category_name(category),
                NSRect::new(NSPoint::new(MARGIN, y), NSSize::new(300.0, 20.0)),
                mtm,
            );
            // SAFETY: the font weight constant is an immutable AppKit value.
            header.setFont(Some(&NSFont::systemFontOfSize_weight(13.0, unsafe {
                NSFontWeightSemibold
            })));
            doc.addSubview(&header);
            y += 26.0;
            for c in &items {
                let mut missing = availability.missing_tools(c);
                missing.extend(availability.missing_engines(c));
                let mut title = catalog.label(c).to_owned();
                // Several conversions with the same label ("Convert to PNG" from HEIC, from
                // SVG…): say which formats each one starts from.
                let shared = items
                    .iter()
                    .any(|o| o.id != c.id && catalog.label(o) == catalog.label(c));
                let formats = c.source_formats();
                if shared && !formats.is_empty() {
                    let shown = if formats.len() > 5 {
                        format!("{}…", formats[..5].join(", "))
                    } else {
                        formats.join(", ")
                    };
                    title = format!(
                        "{title} — {}",
                        i18n.tr_args("prefs-from", &[("formats", Arg::Str(&shown))])
                    );
                }
                if !missing.is_empty() {
                    let tools = missing.join(", ");
                    title = format!(
                        "{title} — {}",
                        i18n.tr_args("prefs-needs", &[("tools", Arg::Str(&tools))])
                    );
                }
                let check = checkbox(&title, self, sel!(conversionToggled:), mtm);
                check.setIdentifier(Some(&NSString::from_str(&c.id)));
                check.setState(if disabled.contains(&c.id) {
                    NSControlStateValueOff
                } else {
                    NSControlStateValueOn
                });
                check.setFrame(NSRect::new(
                    NSPoint::new(MARGIN + 12.0, y),
                    NSSize::new(size.width - 2.0 * MARGIN - 32.0, 22.0),
                ));
                doc.addSubview(&check);
                y += 24.0;
            }
            y += 12.0;
        }
        doc.setFrameSize(NSSize::new(size.width - 20.0, y + MARGIN));

        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), NSRect::new(NSPoint::ZERO, size));
        scroll.setHasVerticalScroller(true);
        scroll.setDrawsBackground(false);
        scroll.setDocumentView(Some(&doc));
        Retained::into_super(scroll)
    }

    fn tools_tab(&self) -> Retained<NSView> {
        let mtm = self.mtm();
        let catalog = self.ivars().view.catalog();
        let i18n = &catalog.i18n;
        let size = Self::content_size();
        let root = FlippedView::new(mtm, size);
        let width = size.width - 2.0 * MARGIN;
        let mut y = MARGIN / 2.0;

        let hint = wrapping(
            &i18n.tr("prefs-tools-hint"),
            NSRect::new(NSPoint::new(MARGIN, y), NSSize::new(width, 48.0)),
            mtm,
        );
        root.addSubview(&hint);
        y += 58.0;

        let mut ffmpeg_missing = false;
        for tool in catalog.required_tools() {
            let text = match catalog.tools.find(&tool) {
                Some(path) => {
                    let path = path.display().to_string();
                    i18n.tr_args(
                        "prefs-tool-found",
                        &[("tool", Arg::Str(&tool)), ("path", Arg::Str(&path))],
                    )
                }
                None => {
                    ffmpeg_missing |= tool == "ffmpeg";
                    format!(
                        "⚠︎ {}",
                        i18n.tr_args("prefs-tool-missing", &[("tool", Arg::Str(&tool))])
                    )
                }
            };
            let row = label(
                &text,
                NSRect::new(NSPoint::new(MARGIN, y), NSSize::new(width, 18.0)),
                mtm,
            );
            row.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(11.0, 0.0)));
            root.addSubview(&row);
            y += 20.0;
        }
        if ffmpeg_missing {
            y += 8.0;
            let hint = wrapping(
                &i18n.tr("prefs-ffmpeg-hint"),
                NSRect::new(NSPoint::new(MARGIN, y), NSSize::new(width, 36.0)),
                mtm,
            );
            hint.setSelectable(true);
            root.addSubview(&hint);
            y += 40.0;
        }

        y += 16.0;
        let issues = catalog.registry.issues();
        let title = label(
            &i18n.tr("prefs-invalid-title"),
            NSRect::new(NSPoint::new(MARGIN, y), NSSize::new(width, 20.0)),
            mtm,
        );
        // SAFETY: the font weight constant is an immutable AppKit value.
        title.setFont(Some(&NSFont::systemFontOfSize_weight(13.0, unsafe {
            NSFontWeightSemibold
        })));
        root.addSubview(&title);
        y += 24.0;
        if issues.is_empty() {
            root.addSubview(&label(
                &i18n.tr("prefs-invalid-none"),
                NSRect::new(NSPoint::new(MARGIN, y), NSSize::new(width, 18.0)),
                mtm,
            ));
            y += 22.0;
        } else {
            for issue in issues.iter().take(6) {
                let name = issue
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let text = wrapping(
                    &format!("{name}: {}", issue.message),
                    NSRect::new(NSPoint::new(MARGIN, y), NSSize::new(width, 34.0)),
                    mtm,
                );
                text.setFont(Some(&NSFont::systemFontOfSize(11.0)));
                text.setSelectable(true);
                root.addSubview(&text);
                y += 38.0;
            }
        }

        // SAFETY: the action is implemented by `self`, the target.
        let rescan = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(&i18n.tr("prefs-rescan")),
                Some(self),
                Some(sel!(rescan:)),
                mtm,
            )
        };
        rescan.setFrame(NSRect::new(
            NSPoint::new(MARGIN, (y + 12.0).min(size.height - 44.0)),
            NSSize::new(130.0, 28.0),
        ));
        root.addSubview(&rescan);
        Retained::into_super(root)
    }
}

fn login_enabled() -> bool {
    // SAFETY: querying the main app service status has no preconditions.
    unsafe { SMAppService::mainAppService().status() == SMAppServiceStatus::Enabled }
}

fn label(text: &str, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    field.setFrame(frame);
    field
}

fn wrapping(text: &str, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    field.setFrame(frame);
    field.setSelectable(false);
    field
}

fn checkbox(
    title: &str,
    target: &PrefsController,
    action: Sel,
    mtm: MainThreadMarker,
) -> Retained<NSButton> {
    // SAFETY: `action` is implemented by `target`, which outlives the window.
    unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str(title),
            Some(target),
            Some(action),
            mtm,
        )
    }
}

fn popup(
    frame: NSRect,
    target: &PrefsController,
    action: Sel,
    mtm: MainThreadMarker,
) -> Retained<NSPopUpButton> {
    let popup = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), frame, false);
    // SAFETY: `action` is implemented by `target`, which outlives the window.
    unsafe {
        popup.setTarget(Some(target));
        popup.setAction(Some(action));
    }
    popup
}
