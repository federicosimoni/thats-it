//! The menu bar item: Preferences, show/hide widget, cancel, folders, About, Quit.

use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAboutPanelOptionApplicationName, NSAboutPanelOptionApplicationVersion,
    NSAboutPanelOptionCredits, NSApplication, NSApplicationDidChangeScreenParametersNotification,
    NSImage, NSMenu, NSMenuDelegate, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength, NSWindow, NSWorkspace,
};
use objc2_foundation::{
    NSAttributedString, NSDictionary, NSNotificationCenter, NSObject, NSObjectProtocol, NSString,
    NSURL,
};

use crate::prefs_window::PrefsController;
use crate::ring_view::RingView;
use crate::{catalog, log, prefs};

pub struct StatusIvars {
    window: Retained<NSWindow>,
    view: Retained<RingView>,
    item: OnceCell<Retained<NSStatusItem>>,
    toggle: OnceCell<Retained<NSMenuItem>>,
    cancel: OnceCell<Retained<NSMenuItem>>,
    prefs: OnceCell<Retained<PrefsController>>,
}

define_class!(
    // SAFETY:
    // - NSObject has no subclassing requirements.
    // - `StatusController` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = StatusIvars]
    pub struct StatusController;

    impl StatusController {
        // SAFETY (all actions): the signature matches an AppKit action method `- (void)x:(id)sender`.
        #[unsafe(method(openPreferences:))]
        fn open_preferences(&self, _sender: Option<&AnyObject>) {
            self.show_preferences();
        }

        #[unsafe(method(toggleWidget:))]
        fn toggle_widget(&self, _sender: Option<&AnyObject>) {
            let iv = self.ivars();
            let hide = iv.window.isVisible();
            prefs::set_widget_hidden(hide);
            if hide {
                iv.window.orderOut(None);
            } else {
                iv.view.apply_preferences();
                iv.window.orderFrontRegardless();
            }
        }

        #[unsafe(method(cancelConversion:))]
        fn cancel_conversion(&self, _sender: Option<&AnyObject>) {
            log::info("cancel requested from the menu bar");
            self.ivars().view.cancel_conversion();
        }

        #[unsafe(method(openConversionsFolder:))]
        fn open_conversions_folder(&self, _sender: Option<&AnyObject>) {
            if let Some(dir) = catalog::user_dir() {
                let _ = std::fs::create_dir_all(&dir);
                open_path(&dir);
            }
        }

        #[unsafe(method(openLogFolder:))]
        fn open_log_folder(&self, _sender: Option<&AnyObject>) {
            if let Some(dir) = log::dir() {
                let _ = std::fs::create_dir_all(&dir);
                open_path(&dir);
            }
        }

        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            let i18n = &self.ivars().view.catalog().i18n;
            let credits = NSAttributedString::from_nsstring(&NSString::from_str(&format!(
                "{}\n\nMIT OR Apache-2.0\ngithub.com/federicosimoni/thats-it",
                i18n.tr("tagline")
            )));
            let name = NSString::from_str("That's it");
            let version = NSString::from_str(env!("CARGO_PKG_VERSION"));
            // SAFETY: the keys are immutable AppKit constants and the values have the
            // documented types (NSString, NSString, NSAttributedString).
            unsafe {
                let options = NSDictionary::<NSString, AnyObject>::from_slices(
                    &[
                        NSAboutPanelOptionApplicationName,
                        NSAboutPanelOptionApplicationVersion,
                        NSAboutPanelOptionCredits,
                    ],
                    &[&*name as &AnyObject, &*version as &AnyObject, &*credits as &AnyObject],
                );
                let app = NSApplication::sharedApplication(self.mtm());
                activate(&app);
                app.orderFrontStandardAboutPanelWithOptions(&options);
            }
        }

        #[unsafe(method(screensChanged:))]
        fn screens_changed(&self, _notification: Option<&AnyObject>) {
            log::info("screen configuration changed");
            self.ivars().view.apply_preferences();
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for StatusController {}

    // SAFETY: the method has the signature declared by `NSMenuDelegate`.
    unsafe impl NSMenuDelegate for StatusController {
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, _menu: &NSMenu) {
            let iv = self.ivars();
            let i18n = &iv.view.catalog().i18n;
            if let Some(toggle) = iv.toggle.get() {
                let id = if iv.window.isVisible() { "menu-hide-widget" } else { "menu-show-widget" };
                toggle.setTitle(&NSString::from_str(&i18n.tr(id)));
            }
            if let Some(cancel) = iv.cancel.get() {
                cancel.setEnabled(iv.view.is_busy());
            }
        }
    }
);

impl StatusController {
    /// Creates the menu bar item. Keep the returned controller alive for the app's lifetime.
    pub fn install(
        mtm: MainThreadMarker,
        window: Retained<NSWindow>,
        view: Retained<RingView>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(StatusIvars {
            window,
            view,
            item: OnceCell::new(),
            toggle: OnceCell::new(),
            cancel: OnceCell::new(),
            prefs: OnceCell::new(),
        });
        // SAFETY: `init` is NSObject's designated initializer.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.build_menu(mtm);
        // SAFETY: `screensChanged:` takes the notification; the controller lives for the
        // whole app run, so it never needs to be removed as an observer.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &this,
                sel!(screensChanged:),
                Some(NSApplicationDidChangeScreenParametersNotification),
                None,
            );
        }
        this
    }

    /// Opens Preferences (menu item, and the CI visual self-test).
    pub fn show_preferences(&self) {
        let iv = self.ivars();
        let prefs = iv
            .prefs
            .get_or_init(|| PrefsController::new(self.mtm(), iv.view.clone()));
        prefs.show();
    }

    fn build_menu(&self, mtm: MainThreadMarker) {
        let iv = self.ivars();
        let i18n = &iv.view.catalog().i18n;
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = item.button(mtm) {
            let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str("tray.and.arrow.down"),
                Some(&NSString::from_str("That's it")),
            );
            if let Some(image) = image {
                image.setTemplate(true);
                button.setImage(Some(&image));
            }
        }

        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("That's it"));
        menu.setAutoenablesItems(false);
        menu.setDelegate(Some(ProtocolObject::from_ref(self)));

        let add = |id: &str, action: Sel, key: &str| -> Retained<NSMenuItem> {
            // SAFETY: the action is implemented by `self`, which is set as the target.
            let entry = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(&i18n.tr(id)),
                    Some(action),
                    &NSString::from_str(key),
                )
            };
            // SAFETY: `self` lives as long as the app and responds to `action`.
            unsafe { entry.setTarget(Some(self)) };
            menu.addItem(&entry);
            entry
        };
        add("menu-preferences", sel!(openPreferences:), ",");
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let toggle = add("menu-hide-widget", sel!(toggleWidget:), "");
        let cancel = add("menu-cancel", sel!(cancelConversion:), "");
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add("menu-open-conversions", sel!(openConversionsFolder:), "");
        add("menu-open-log", sel!(openLogFolder:), "");
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add("menu-about", sel!(showAbout:), "");
        add("menu-quit", sel!(quit:), "q");
        item.setMenu(Some(&menu));

        let _ = iv.item.set(item);
        let _ = iv.toggle.set(toggle);
        let _ = iv.cancel.set(cancel);
    }
}

fn open_path(path: &std::path::Path) {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSWorkspace::sharedWorkspace().openURL(&url);
}

/// Brings this (accessory) app to the front so its windows get focus.
pub fn activate(app: &NSApplication) {
    // `activate` exists from macOS 14; Ventura needs the older call.
    if app.respondsToSelector(sel!(activate)) {
        app.activate();
    } else {
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}
