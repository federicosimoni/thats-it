//! The Finder service "Convert with That's it…" (right-click → Services / Quick Actions).
//!
//! Declared in `Info.plist` (`NSServices`, message `convertFiles`). macOS sends the
//! selected files on a pasteboard; we show a menu of the compatible conversions at the
//! mouse location and run the chosen one exactly like a drop on the widget. Works even
//! when the desktop is covered by windows or a full-screen app.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSEvent, NSMenu, NSMenuItem, NSPasteboard};
use objc2_foundation::{NSObjectProtocol, NSString};
use thatsit_core::conversion::Category;

use crate::catalog::Catalog;
use crate::files::{self, DraggedFile};
use crate::{log, ring_view, status};

pub struct ServiceIvars {
    catalog: Rc<Catalog>,
    /// Files of the service request whose menu is open.
    pending: RefCell<Vec<DraggedFile>>,
}

define_class!(
    // SAFETY:
    // - NSObject has no subclassing requirements.
    // - `ServiceProvider` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ServiceIvars]
    pub struct ServiceProvider;

    impl ServiceProvider {
        // SAFETY: the signature matches an NSServices handler:
        // `- (void)convertFiles:(NSPasteboard *)pboard userData:(NSString *)data
        //   error:(NSString **)error`.
        #[unsafe(method(convertFiles:userData:error:))]
        fn convert_files(
            &self,
            pasteboard: &NSPasteboard,
            _user_data: Option<&NSString>,
            _error: *mut *mut NSString,
        ) {
            let dragged = files::read_files(pasteboard);
            log::info(format!("Finder service with {} file(s)", dragged.len()));
            self.show_menu(dragged);
        }

        // SAFETY: the signature matches an AppKit action method `- (void)x:(id)sender`.
        #[unsafe(method(chooseConversion:))]
        fn choose_conversion(&self, sender: Option<&NSMenuItem>) {
            let Some(id) = sender
                .and_then(|item| item.representedObject())
                .and_then(|o| o.downcast::<NSString>().ok())
            else {
                return;
            };
            let id = id.to_string();
            let files: Vec<PathBuf> = self
                .ivars()
                .pending
                .take()
                .into_iter()
                .map(|f| PathBuf::from(f.path))
                .collect();
            log::info(format!("Finder service → run {id} on {} file(s)", files.len()));
            ring_view::with_view(|view| {
                view.run_conversion(&id, files);
            });
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for ServiceProvider {}
);

impl ServiceProvider {
    /// Registers the provider with AppKit. Keep the returned object alive.
    pub fn install(mtm: MainThreadMarker, catalog: Rc<Catalog>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ServiceIvars {
            catalog,
            pending: RefCell::new(Vec::new()),
        });
        // SAFETY: `init` is NSObject's designated initializer.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: `this` implements the message named in Info.plist (`convertFiles`) and
        // lives for the whole app run (the caller keeps it).
        unsafe { app.setServicesProvider(Some(&this)) };
        objc2_app_kit::NSUpdateDynamicServices();
        this
    }

    fn show_menu(&self, dragged: Vec<DraggedFile>) {
        let mtm = self.mtm();
        let catalog = &self.ivars().catalog;
        let matches = catalog.compatible(&dragged);
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("That's it"));
        menu.setAutoenablesItems(false);

        let disabled_item = |title: &str| {
            let item = NSMenuItem::new(mtm);
            item.setTitle(&NSString::from_str(title));
            item.setEnabled(false);
            item
        };
        if matches.is_empty() {
            menu.addItem(&disabled_item(&catalog.i18n.tr("service-no-match")));
        }
        for category in Category::ALL {
            let in_category: Vec<_> = matches.iter().filter(|c| c.category == category).collect();
            if in_category.is_empty() {
                continue;
            }
            if menu.numberOfItems() > 0 {
                menu.addItem(&NSMenuItem::separatorItem(mtm));
            }
            menu.addItem(&disabled_item(&catalog.category_name(category)));
            for c in in_category {
                // SAFETY: `chooseConversion:` is implemented by `self`, set as the target.
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(catalog.label(c)),
                        Some(sel!(chooseConversion:)),
                        &NSString::from_str(""),
                    )
                };
                // SAFETY: `self` lives as long as the app and responds to the action.
                unsafe { item.setTarget(Some(self)) };
                let id = NSString::from_str(&c.id);
                // SAFETY: an NSString is a valid represented object.
                unsafe { item.setRepresentedObject(Some(&*id as &AnyObject)) };
                item.setIndentationLevel(1);
                menu.addItem(&item);
            }
        }

        *self.ivars().pending.borrow_mut() = dragged;
        // An accessory app must be active for its menu to track the mouse.
        status::activate(&NSApplication::sharedApplication(mtm));
        menu.popUpMenuPositioningItem_atLocation_inView(None, NSEvent::mouseLocation(), None);
    }
}
