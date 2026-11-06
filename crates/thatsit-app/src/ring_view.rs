//! The widget view: glass disc (collapsed) that expands into the radial menu while files
//! are dragged over it.
//!
//! Layering, back to front:
//! 1. `glass` — an `NSVisualEffectView` (behind-window blending) clipped to a circle;
//! 2. `shapes` — a layer-backed view hosting one `CAShapeLayer` per sector + the centre;
//! 3. `labels` — SF Symbol image views and text fields for each sector, the centre glyph
//!    and the collapsed glyph.
//!
//! All behaviour (hover delay, rings, dwell, drop outcome) lives in
//! `thatsit_core::menu`; this view only renders it.

use std::cell::{Cell, OnceCell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use dispatch2::{DispatchQueue, DispatchTime};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSAppearanceCustomization,
    NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSColor, NSColorSpace, NSDragOperation,
    NSDraggingDestination, NSDraggingInfo, NSEvent, NSFont, NSFontWeightMedium,
    NSFontWeightRegular, NSFontWeightSemibold, NSImage, NSImageSymbolConfiguration, NSImageView,
    NSPasteboardTypeFileURL, NSScreen, NSTextAlignment, NSTextField, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindowDelegate,
};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{CGColor, CGMutablePath};
use objc2_foundation::{
    NSArray, NSNotification, NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use objc2_quartz_core::{
    CABasicAnimation, CALayer, CAMediaTiming, CAMediaTimingFunction, CAShapeLayer, CATransaction,
    kCAMediaTimingFunctionEaseOut,
};
use thatsit_core::conversion::Category;
use thatsit_core::geometry::{self, Hit, RingLayout};
use thatsit_core::menu::{DropOutcome, Entry, Interaction, Level, Menu, ViewState};

use crate::catalog::Catalog;
use crate::files::{self, DraggedFile};
use crate::jobs::{Batch, Event, FileResult, Jobs};
use crate::notify;
use crate::{log, prefs};

/// Side of the window while collapsed, in points.
pub const COLLAPSED_SIZE: f64 = 80.0;
/// Diameter of the collapsed glass disc.
const COLLAPSED_DISC: f64 = 76.0;
/// Side of the window while expanded, in points.
pub const EXPANDED_SIZE: f64 = 332.0;
/// Radius of the expanded glass disc.
const GLASS_RADIUS: f64 = 160.0;
/// Sector band, inner and outer radius.
const SECTOR_INNER: f64 = 44.0;
const SECTOR_OUTER: f64 = 154.0;
/// Hit-testing: dead zone and (forgiving) outer radius.
const DEAD_ZONE: f64 = 42.0;
const HIT_OUTER: f64 = EXPANDED_SIZE / 2.0;
/// Duration of the expand animation.
const EXPAND_SECONDS: f64 = 0.16;

fn now() -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// A conversion that matches the dragged files, as shown in the ring.
#[derive(Debug, Clone)]
struct Match {
    id: String,
    label: String,
    icon: Option<String>,
    category: Category,
}

/// One drag over the widget.
struct Session {
    files: Vec<DraggedFile>,
    matches: Vec<Match>,
    interaction: Interaction,
    /// The level currently built into layers/views.
    built: Option<Level>,
    last: Option<ViewState>,
}

/// Shape layer, icon and label of one sector.
type SectorViews = (
    Retained<CAShapeLayer>,
    Retained<NSImageView>,
    Retained<NSTextField>,
);

pub struct RingIvars {
    catalog: Rc<Catalog>,
    glass: OnceCell<Retained<NSVisualEffectView>>,
    shapes: OnceCell<Retained<NSView>>,
    ring_layer: OnceCell<Retained<CALayer>>,
    centre: OnceCell<Retained<CAShapeLayer>>,
    labels: OnceCell<Retained<NSView>>,
    glyph: OnceCell<Retained<NSImageView>>,
    centre_glyph: OnceCell<Retained<NSImageView>>,
    /// Progress ring around the collapsed disc while converting.
    progress: OnceCell<Retained<CAShapeLayer>>,
    jobs: Jobs,
    /// (done, total) of the running batch, if any.
    busy: Cell<Option<(usize, usize)>>,
    /// Glyph shown briefly after a batch (success / failure), with a generation counter
    /// so a stale timer does not clear a newer one.
    flash: Cell<Option<(&'static str, u64)>>,
    flash_gen: Cell<u64>,
    sectors: RefCell<Vec<SectorViews>>,
    session: RefCell<Option<Session>>,
    expanded: Cell<bool>,
    unsupported: Cell<bool>,
    /// Set while we move/resize the window ourselves (ignore `windowDidMove:`).
    resizing: Cell<bool>,
}

define_class!(
    // SAFETY:
    // - NSView has no subclassing requirements beyond main-thread use, which
    //   `thread_kind = MainThreadOnly` enforces.
    // - `RingView` does not implement `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RingIvars]
    pub struct RingView;

    impl RingView {
        // Clicks on an inactive (accessory) app's window are handled immediately.
        // SAFETY: the signature matches `-[NSView acceptsFirstMouse:]`.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        // Dragging the disc moves the window.
        // SAFETY: the signature matches `-[NSView mouseDownCanMoveWindow]`.
        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            true
        }

        // Only the disc is hit; the transparent corners let clicks through to the desktop.
        // All points inside resolve to this view so subviews never steal drags or clicks.
        // SAFETY: the signature matches `-[NSView hitTest:]`.
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
            let f = self.frame();
            let local = NSPoint::new(point.x - f.origin.x, point.y - f.origin.y);
            let c = centre_of(self.bounds());
            let r = if self.ivars().expanded.get() { GLASS_RADIUS } else { COLLAPSED_DISC / 2.0 };
            let inside = (local.x - c.x).hypot(local.y - c.y) <= r;
            inside.then(|| Retained::into_super(self.retain()))
        }

        // SAFETY: the signature matches `-[NSView viewDidChangeEffectiveAppearance]`.
        #[unsafe(method(viewDidChangeEffectiveAppearance))]
        fn view_did_change_effective_appearance(&self) {
            self.refresh_colors();
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for RingView {}

    // SAFETY: the method has the signature declared by `NSWindowDelegate`.
    unsafe impl NSWindowDelegate for RingView {
        #[unsafe(method(windowDidMove:))]
        fn window_did_move(&self, _notification: &NSNotification) {
            if !self.ivars().resizing.get() && !self.ivars().expanded.get() {
                self.save_position();
            }
        }
    }

    // SAFETY: every method below has the exact signature declared by
    // `NSDraggingDestination`.
    unsafe impl NSDraggingDestination for RingView {
        #[unsafe(method(draggingEntered:))]
        fn dragging_entered(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            let dragged = files::read_files(&sender.draggingPasteboard());
            self.begin_session(dragged)
        }

        #[unsafe(method(draggingUpdated:))]
        fn dragging_updated(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            let location = sender.draggingLocation();
            self.track(location)
        }

        #[unsafe(method(draggingExited:))]
        fn dragging_exited(&self, _sender: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
            if self.ivars().session.borrow().is_some() {
                log::info("drag exited → collapse");
            }
            self.end_session();
        }

        #[unsafe(method(draggingEnded:))]
        fn dragging_ended(&self, _sender: &ProtocolObject<dyn NSDraggingInfo>) {
            self.end_session();
        }

        #[unsafe(method(prepareForDragOperation:))]
        fn prepare_for_drag_operation(&self, _sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            self.ivars().session.borrow().is_some()
        }

        #[unsafe(method(performDragOperation:))]
        fn perform_drag_operation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            let hit = self.hit_at(sender.draggingLocation());
            let run = self.drop_at(hit);
            self.end_session();
            run
        }
    }
);

impl RingView {
    pub fn new(mtm: MainThreadMarker, catalog: Rc<Catalog>) -> Retained<Self> {
        let frame = NSRect::new(NSPoint::ZERO, NSSize::new(COLLAPSED_SIZE, COLLAPSED_SIZE));
        let jobs = Jobs::start(Arc::clone(&catalog.tools), on_job_event);
        let this = Self::alloc(mtm).set_ivars(RingIvars {
            catalog,
            glass: OnceCell::new(),
            shapes: OnceCell::new(),
            ring_layer: OnceCell::new(),
            centre: OnceCell::new(),
            labels: OnceCell::new(),
            glyph: OnceCell::new(),
            centre_glyph: OnceCell::new(),
            progress: OnceCell::new(),
            jobs,
            busy: Cell::new(None),
            flash: Cell::new(None),
            flash_gen: Cell::new(0),
            sectors: RefCell::new(Vec::new()),
            session: RefCell::new(None),
            expanded: Cell::new(false),
            unsupported: Cell::new(false),
            resizing: Cell::new(false),
        });
        // SAFETY: `initWithFrame:` is NSView's designated initializer and takes an NSRect.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.setWantsLayer(true);
        view.build_subviews(mtm);
        // SAFETY: `NSPasteboardTypeFileURL` is an immutable NSString constant exported by AppKit.
        let file_url_type = unsafe { NSPasteboardTypeFileURL };
        view.registerForDraggedTypes(&NSArray::from_slice(&[file_url_type]));
        view.layout_subviews();
        view.refresh_colors();
        view
    }

    // -----------------------------------------------------------------------------------
    // Construction and layout

    fn build_subviews(&self, mtm: MainThreadMarker) {
        let bounds = self.bounds();

        let glass = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), bounds);
        glass.setMaterial(NSVisualEffectMaterial::Popover);
        glass.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        glass.setState(NSVisualEffectState::Active);
        glass.setWantsLayer(true);
        self.addSubview(&glass);

        let shapes = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        shapes.setWantsLayer(true);
        let ring_layer = CALayer::new();
        if let Some(host) = shapes.layer() {
            host.addSublayer(&ring_layer);
        }
        let centre = CAShapeLayer::new();
        ring_layer.addSublayer(&centre);
        let progress = CAShapeLayer::new();
        progress.setFillColor(None);
        progress.setLineWidth(3.0);
        progress.setLineCap(&NSString::from_str("round"));
        progress.setStrokeEnd(0.0);
        progress.setHidden(true);
        if let Some(host) = shapes.layer() {
            host.addSublayer(&progress);
        }
        self.addSubview(&shapes);

        let labels = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        let glyph = symbol_view("tray.and.arrow.down", 30.0, mtm);
        labels.addSubview(&glyph);
        let centre_glyph = symbol_view("xmark", 15.0, mtm);
        centre_glyph.setHidden(true);
        labels.addSubview(&centre_glyph);
        self.addSubview(&labels);

        let iv = self.ivars();
        let _ = iv.glass.set(glass);
        let _ = iv.shapes.set(shapes);
        let _ = iv.ring_layer.set(ring_layer);
        let _ = iv.centre.set(centre);
        let _ = iv.labels.set(labels);
        let _ = iv.glyph.set(glyph);
        let _ = iv.centre_glyph.set(centre_glyph);
        let _ = iv.progress.set(progress);
    }

    /// Positions every subview for the current size and state.
    fn layout_subviews(&self) {
        let iv = self.ivars();
        let bounds = self.bounds();
        let c = centre_of(bounds);
        let expanded = iv.expanded.get();

        without_animations(|| {
            let radius = if expanded {
                GLASS_RADIUS
            } else {
                COLLAPSED_DISC / 2.0
            };
            if let Some(glass) = iv.glass.get() {
                glass.setFrame(square_around(c, radius * 2.0));
                if let Some(layer) = glass.layer() {
                    layer.setCornerRadius(radius);
                    layer.setMasksToBounds(true);
                }
            }
            if let Some(shapes) = iv.shapes.get() {
                shapes.setFrame(bounds);
            }
            if let Some(ring) = iv.ring_layer.get() {
                ring.setBounds(bounds);
                ring.setPosition(c);
                ring.setHidden(!expanded);
            }
            if let Some(centre) = iv.centre.get() {
                let path = CGMutablePath::new();
                // SAFETY: a null transform pointer is explicitly allowed by CGPath.
                unsafe {
                    CGMutablePath::add_ellipse_in_rect(
                        Some(&path),
                        std::ptr::null(),
                        square_around(c, DEAD_ZONE * 2.0 - 6.0),
                    );
                }
                centre.setPath(Some(&path));
            }
            if let Some(labels) = iv.labels.get() {
                labels.setFrame(bounds);
            }
            if let Some(glyph) = iv.glyph.get() {
                glyph.setFrame(square_around(c, 40.0));
                glyph.setHidden(expanded);
            }
            if let Some(g) = iv.centre_glyph.get() {
                g.setFrame(square_around(c, 24.0));
                g.setHidden(!expanded);
            }
            if let Some(progress) = iv.progress.get() {
                // Starts at 12 o'clock and runs clockwise.
                let path = CGMutablePath::new();
                let r = COLLAPSED_DISC / 2.0 - 2.5;
                let top = std::f64::consts::FRAC_PI_2;
                // SAFETY: a null transform pointer is explicitly allowed by CGPath.
                unsafe {
                    CGMutablePath::add_arc(
                        Some(&path),
                        std::ptr::null(),
                        c.x,
                        c.y,
                        r,
                        top,
                        top - std::f64::consts::TAU,
                        true,
                    );
                }
                progress.setPath(Some(&path));
                progress.setHidden(expanded || iv.busy.get().is_none());
            }
        });
    }

    /// Rebuilds sector layers and labels for `entries`.
    fn build_ring(&self, entries: &[Entry], matches: &[Match], level: Level) {
        let iv = self.ivars();
        let mtm = self.mtm();
        let (Some(ring), Some(labels)) = (iv.ring_layer.get(), iv.labels.get()) else {
            return;
        };
        let c = centre_of(self.bounds());
        let layout = RingLayout::new(entries.len(), DEAD_ZONE, HIT_OUTER);
        let italian = iv.catalog.prefers_italian();

        without_animations(|| {
            for (shape, icon, text) in iv.sectors.borrow_mut().drain(..) {
                shape.removeFromSuperlayer();
                icon.removeFromSuperview();
                text.removeFromSuperview();
            }
            let mut sectors = iv.sectors.borrow_mut();
            for (i, entry) in entries.iter().enumerate() {
                let (start, end) = layout.sector_span(i);
                let shape = CAShapeLayer::new();
                shape.setPath(Some(&sector_path(
                    c,
                    SECTOR_INNER,
                    SECTOR_OUTER,
                    start,
                    end,
                )));
                ring.addSublayer(&shape);

                let (symbol, label) = match entry {
                    Entry::Conversion(m) => {
                        let m = &matches[*m];
                        (
                            m.icon.clone().unwrap_or_else(|| "doc".into()),
                            m.label.clone(),
                        )
                    }
                    Entry::Category { category, count } => (
                        category_symbol(*category).to_owned(),
                        format!("{} ({count})", category_name(*category, italian)),
                    ),
                };
                // Label centre: middle of the band, at the sector's centre angle.
                let mid = (SECTOR_INNER + SECTOR_OUTER) / 2.0;
                let angle = ((start + end) / 2.0).to_radians();
                let p = NSPoint::new(c.x + mid * angle.sin(), c.y + mid * angle.cos());

                let icon = symbol_view(&symbol, 18.0, mtm);
                icon.setFrame(NSRect::new(
                    NSPoint::new(p.x - 14.0, p.y + 2.0),
                    NSSize::new(28.0, 24.0),
                ));
                labels.addSubview(&icon);

                let text = NSTextField::wrappingLabelWithString(&NSString::from_str(&label), mtm);
                text.setAlignment(NSTextAlignment::Center);
                text.setMaximumNumberOfLines(2);
                text.setSelectable(false);
                text.setFrame(NSRect::new(
                    NSPoint::new(p.x - 48.0, p.y - 32.0),
                    NSSize::new(96.0, 32.0),
                ));
                labels.addSubview(&text);

                sectors.push((shape, icon, text));
            }
        });

        if let Some(g) = iv.centre_glyph.get() {
            let back = matches!(level, Level::Category(_));
            set_symbol(g, if back { "chevron.backward" } else { "xmark" }, 15.0);
        }
    }

    /// Applies colors for the current appearance, preference and highlight.
    fn refresh_colors(&self) {
        let iv = self.ivars();
        let palette = Palette::new(self.is_dark(), prefs::use_system_accent());
        let view = iv.session.borrow().as_ref().and_then(|s| s.last);
        let highlight = view.and_then(|v| v.highlight);
        let centre_on = view.is_some_and(|v| v.centre);

        let medium = font_weight(Weight::Medium);
        let semibold = font_weight(Weight::Semibold);
        for (i, (shape, icon, text)) in iv.sectors.borrow().iter().enumerate() {
            let on = highlight == Some(i);
            let (fill, stroke, width) = if on {
                (&palette.hl_fill, &palette.hl_stroke, palette.hl_width)
            } else {
                (&palette.sector, &palette.sector_stroke, 1.0)
            };
            shape.setFillColor(Some(fill));
            shape.setStrokeColor(Some(stroke));
            shape.setLineWidth(width);
            let color = if on { &palette.hl_text } else { &palette.text };
            icon.setContentTintColor(Some(color));
            text.setTextColor(Some(color));
            text.setFont(Some(&NSFont::systemFontOfSize_weight(
                12.0,
                if on { semibold } else { medium },
            )));
        }
        if let Some(centre) = iv.centre.get() {
            centre.setFillColor(Some(if centre_on {
                &palette.centre_on
            } else {
                &palette.centre
            }));
        }
        if let Some(g) = iv.centre_glyph.get() {
            g.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
        }
        if let Some(progress) = iv.progress.get() {
            progress.setStrokeColor(Some(&palette.progress));
        }
        if let Some(g) = iv.glyph.get() {
            let color = if iv.unsupported.get() {
                NSColor::tertiaryLabelColor()
            } else {
                NSColor::secondaryLabelColor()
            };
            g.setContentTintColor(Some(&color));
        }
    }

    fn is_dark(&self) -> bool {
        // SAFETY: both names are immutable NSString constants exported by AppKit.
        let (aqua, dark) = unsafe { (NSAppearanceNameAqua, NSAppearanceNameDarkAqua) };
        self.effectiveAppearance()
            .bestMatchFromAppearancesWithNames(&NSArray::from_slice(&[aqua, dark]))
            .is_some_and(|name| &*name == dark)
    }

    // -----------------------------------------------------------------------------------
    // Drag session

    fn begin_session(&self, dragged: Vec<DraggedFile>) -> NSDragOperation {
        let iv = self.ivars();
        if dragged.is_empty() {
            log::info("drag entered with no file URLs → not supported");
            self.set_unsupported(true);
            return NSDragOperation::None;
        }
        let infos: Vec<_> = dragged.iter().map(DraggedFile::file_info).collect();
        let matches: Vec<Match> = iv
            .catalog
            .registry
            .matching(&infos, &iv.catalog.availability)
            .into_iter()
            .map(|c| Match {
                id: c.id.clone(),
                label: iv.catalog.label(c).to_owned(),
                icon: c.icon.clone(),
                category: c.category,
            })
            .collect();

        log::info(format!("drag entered with {} file(s):", dragged.len()));
        for f in &dragged {
            log::info(format!(
                "    {}  [{}]",
                f.path,
                f.uti.as_deref().unwrap_or("unknown UTType")
            ));
        }
        if matches.is_empty() {
            log::info("no compatible conversion → not supported");
            self.set_unsupported(true);
            return NSDragOperation::None;
        }
        log::info(format!(
            "compatible: {}",
            matches
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));

        let categories: Vec<Category> = matches.iter().map(|m| m.category).collect();
        let menu = Menu::new(&categories);
        if menu.hidden_count() > 0 {
            log::info(format!(
                "{} conversion(s) do not fit in the ring and are not shown",
                menu.hidden_count()
            ));
        }
        *iv.session.borrow_mut() = Some(Session {
            files: dragged,
            matches,
            interaction: Interaction::new(menu, now()),
            built: None,
            last: None,
        });
        NSDragOperation::Copy
    }

    /// Handles a drag position (window coordinates). Returns the drag operation.
    fn track(&self, location: NSPoint) -> NSDragOperation {
        if self.ivars().session.borrow().is_none() {
            return NSDragOperation::None;
        }
        let hit = self.hit_at(location);
        self.apply(hit, now());
        NSDragOperation::Copy
    }

    /// Feeds a hit to the interaction and updates the view.
    fn apply(&self, hit: Hit, at: Duration) {
        let iv = self.ivars();
        let (view, rebuild) = {
            let mut session = iv.session.borrow_mut();
            let Some(s) = session.as_mut() else {
                return;
            };
            let view = s.interaction.update(hit, at);
            let changed = s.last != Some(view);
            if changed {
                log_view(s, &view);
            }
            s.last = Some(view);
            let rebuild = (view.expanded && s.built != Some(view.level)).then(|| {
                s.built = Some(view.level);
                (s.interaction.entries(), s.matches.clone(), view.level)
            });
            (view, rebuild)
        };
        if view.expanded && !iv.expanded.get() {
            self.set_expanded(true);
        }
        if let Some((entries, matches, level)) = rebuild {
            self.build_ring(&entries, &matches, level);
        }
        self.refresh_colors();
    }

    fn drop_at(&self, hit: Hit) -> bool {
        let session = self.ivars().session.borrow();
        let Some(s) = session.as_ref() else {
            return false;
        };
        match s.interaction.drop_at(hit) {
            DropOutcome::Run(i) => {
                let m = &s.matches[i];
                log::info(format!(
                    "DROP → run {} ({}) on {} file(s)",
                    m.id,
                    m.label,
                    s.files.len()
                ));
                let iv = self.ivars();
                let Some(conversion) = iv.catalog.registry.get(&m.id).cloned() else {
                    return false;
                };
                iv.jobs.submit(Batch {
                    conversion,
                    label: m.label.clone(),
                    files: s.files.iter().map(|f| PathBuf::from(&f.path)).collect(),
                });
                true
            }
            DropOutcome::Cancel => {
                log::info("DROP → cancelled");
                false
            }
        }
    }

    fn end_session(&self) {
        let iv = self.ivars();
        iv.session.borrow_mut().take();
        self.set_unsupported(false);
        self.set_expanded(false);
    }

    fn set_unsupported(&self, unsupported: bool) {
        let iv = self.ivars();
        if iv.unsupported.get() == unsupported {
            return;
        }
        iv.unsupported.set(unsupported);
        self.update_glyph();
        self.refresh_colors();
    }

    /// The collapsed glyph: not-supported, a brief result flash, or the normal tray.
    fn update_glyph(&self) {
        let iv = self.ivars();
        let name = if iv.unsupported.get() {
            "nosign"
        } else if let Some((name, _)) = iv.flash.get() {
            name
        } else {
            "tray.and.arrow.down"
        };
        if let Some(g) = iv.glyph.get() {
            set_symbol(g, name, 30.0);
        }
    }

    // -----------------------------------------------------------------------------------
    // Jobs

    fn job_event(&self, event: Event) {
        let iv = self.ivars();
        match event {
            Event::Progress { done, total } => {
                iv.busy.set((done < total).then_some((done, total)));
                if let Some(p) = iv.progress.get() {
                    // Show a sliver at the start so the ring is visible immediately.
                    let fraction = if total == 0 {
                        1.0
                    } else {
                        done as f64 / total as f64
                    };
                    p.setStrokeEnd(fraction.max(0.04));
                    p.setHidden(iv.expanded.get() || iv.busy.get().is_none());
                }
            }
            Event::Finished { label, results } => {
                iv.busy.set(None);
                if let Some(p) = iv.progress.get() {
                    p.setHidden(true);
                    p.setStrokeEnd(0.0);
                }
                let summary = summarize(&label, &results, iv.catalog.prefers_italian());
                self.flash(if summary.failed > 0 {
                    "exclamationmark.triangle"
                } else if summary.ok > 0 {
                    "checkmark.circle"
                } else {
                    "xmark.circle"
                });
                notify::post("That's it", &summary.text);
            }
        }
    }

    fn flash(&self, glyph: &'static str) {
        let iv = self.ivars();
        let generation = iv.flash_gen.get() + 1;
        iv.flash_gen.set(generation);
        iv.flash.set(Some((glyph, generation)));
        self.update_glyph();
        if let Ok(when) = DispatchTime::try_from(Duration::from_millis(2500)) {
            let _ = DispatchQueue::main().after(when, move || {
                with_view(|v| {
                    if v.ivars().flash.get().is_some_and(|(_, g)| g == generation) {
                        v.ivars().flash.set(None);
                        v.update_glyph();
                    }
                })
            });
        }
    }

    /// Hit-test a point in window coordinates against the current ring.
    fn hit_at(&self, location: NSPoint) -> Hit {
        let iv = self.ivars();
        if !iv.expanded.get() {
            return Hit::Outside;
        }
        let count = iv.sectors.borrow().len().max(1);
        let p = self.convertPoint_fromView(location, None);
        let c = centre_of(self.bounds());
        RingLayout::new(count, DEAD_ZONE, HIT_OUTER).hit_test(p.x - c.x, p.y - c.y)
    }

    // -----------------------------------------------------------------------------------
    // Window size and position

    /// Grows (centred, clamped into the screen) or shrinks the window.
    fn set_expanded(&self, expanded: bool) {
        let iv = self.ivars();
        if iv.expanded.get() == expanded {
            return;
        }
        let Some(window) = self.window() else {
            return;
        };
        let mut centre = centre_of(window.frame());
        if expanded {
            if let Some(screen) = window.screen().or_else(|| NSScreen::mainScreen(self.mtm())) {
                let (x, y) = geometry::clamp_center(
                    (centre.x, centre.y),
                    EXPANDED_SIZE / 2.0,
                    to_rect(screen.visibleFrame()),
                );
                centre = NSPoint::new(x, y);
            }
        }
        let size = if expanded {
            EXPANDED_SIZE
        } else {
            COLLAPSED_SIZE
        };
        iv.expanded.set(expanded);
        iv.resizing.set(true);
        window.setFrame_display(square_around(centre, size), true);
        iv.resizing.set(false);
        if !expanded {
            for (shape, icon, text) in iv.sectors.borrow_mut().drain(..) {
                shape.removeFromSuperlayer();
                icon.removeFromSuperview();
                text.removeFromSuperview();
            }
            // The collapsed position may have moved because of clamping.
            self.save_position();
        }
        self.layout_subviews();
        if expanded {
            self.animate_expand();
        }
    }

    fn animate_expand(&self) {
        let iv = self.ivars();
        if let Some(ring) = iv.ring_layer.get() {
            let scale = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str(
                "transform.scale",
            )));
            let fade = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str("opacity")));
            let from_scale = NSNumber::new_f64(0.6);
            let from_opacity = NSNumber::new_f64(0.0);
            let to = NSNumber::new_f64(1.0);
            // SAFETY: both key paths animate scalar values, so NSNumber is the right type.
            unsafe {
                scale.setFromValue(Some(&*from_scale as &AnyObject));
                scale.setToValue(Some(&*to as &AnyObject));
                fade.setFromValue(Some(&*from_opacity as &AnyObject));
                fade.setToValue(Some(&*to as &AnyObject));
            }
            // SAFETY: `kCAMediaTimingFunctionEaseOut` is an immutable constant from QuartzCore.
            let ease =
                CAMediaTimingFunction::functionWithName(unsafe { kCAMediaTimingFunctionEaseOut });
            for anim in [&scale, &fade] {
                anim.setDuration(EXPAND_SECONDS);
                anim.setTimingFunction(Some(&ease));
            }
            ring.addAnimation_forKey(&scale, Some(&NSString::from_str("expand-scale")));
            ring.addAnimation_forKey(&fade, Some(&NSString::from_str("expand-fade")));
        }
        if let Some(labels) = iv.labels.get() {
            labels.setAlphaValue(0.0);
            NSAnimationContext::beginGrouping();
            NSAnimationContext::currentContext().setDuration(EXPAND_SECONDS);
            labels.animator().setAlphaValue(1.0);
            NSAnimationContext::endGrouping();
        }
    }

    /// Places the collapsed widget at its saved position on `screen` (or the default).
    pub fn place_initially(&self) {
        let Some(window) = self.window() else {
            return;
        };
        let Some(screen) = NSScreen::mainScreen(self.mtm()) else {
            return;
        };
        let vf = screen.visibleFrame();
        let name = screen.localizedName().to_string();
        let (fx, fy) = prefs::position(&name).unwrap_or_else(|| {
            // Default: bottom-right corner, with room for the expanded ring.
            let inset = EXPANDED_SIZE / 2.0 + 24.0;
            (1.0 - inset / vf.size.width, inset / vf.size.height)
        });
        let centre = (
            vf.origin.x + fx * vf.size.width,
            vf.origin.y + fy * vf.size.height,
        );
        let (x, y) = geometry::clamp_center(centre, EXPANDED_SIZE / 2.0, to_rect(vf));
        self.ivars().resizing.set(true);
        window.setFrame_display(square_around(NSPoint::new(x, y), COLLAPSED_SIZE), true);
        self.ivars().resizing.set(false);
        log::info(format!("widget on screen {name:?} at ({x:.0}, {y:.0})"));
    }

    fn save_position(&self) {
        let Some(window) = self.window() else {
            return;
        };
        let Some(screen) = window.screen() else {
            return;
        };
        let vf = screen.visibleFrame();
        let c = centre_of(window.frame());
        prefs::set_position(
            &screen.localizedName().to_string(),
            (c.x - vf.origin.x) / vf.size.width,
            (c.y - vf.origin.y) / vf.size.height,
        );
    }

    // -----------------------------------------------------------------------------------
    // Visual self-test

    /// Simulates a drag of `paths` hovering the first sector (for CI screenshots).
    pub fn start_demo(&self, paths: &[String], mouse: bool) {
        let dragged: Vec<_> = paths.iter().map(|p| files::demo_file(p)).collect();
        if self.begin_session(dragged) == NSDragOperation::None {
            return;
        }
        let t = now();
        self.apply(Hit::Outside, t + thatsit_core::menu::HOVER_DELAY);
        self.apply(
            if mouse { Hit::Sector(0) } else { Hit::DeadZone },
            t + thatsit_core::menu::HOVER_DELAY + Duration::from_millis(10),
        );
    }
}

fn log_view(s: &Session, view: &ViewState) {
    if !view.expanded {
        return;
    }
    let what = if view.centre {
        match view.level {
            Level::Root => "centre (drop = cancel)".to_owned(),
            Level::Category(_) => "centre (rest = back, drop = cancel)".to_owned(),
        }
    } else if let Some(i) = view.highlight {
        match s.interaction.entries().get(i) {
            Some(Entry::Conversion(m)) => format!("sector {i}: {}", s.matches[*m].id),
            Some(Entry::Category { category, count }) => {
                format!("sector {i}: category {} ({count})", category.as_str())
            }
            None => format!("sector {i}"),
        }
    } else {
        "outside".to_owned()
    };
    let level = match view.level {
        Level::Root => "root".to_owned(),
        Level::Category(c) => c.as_str().to_owned(),
    };
    log::info(format!("[{level}] {what}"));
}

// ---------------------------------------------------------------------------------------
// Colors

struct Palette {
    sector: CFRetained<CGColor>,
    sector_stroke: CFRetained<CGColor>,
    hl_fill: CFRetained<CGColor>,
    hl_stroke: CFRetained<CGColor>,
    hl_width: f64,
    centre: CFRetained<CGColor>,
    centre_on: CFRetained<CGColor>,
    text: Retained<NSColor>,
    hl_text: Retained<NSColor>,
    progress: CFRetained<CGColor>,
}

impl Palette {
    fn new(dark: bool, accent: bool) -> Self {
        let rgba = |r: f64, g: f64, b: f64, a: f64| CGColor::new_srgb(r, g, b, a);
        let (sector, sector_stroke, glass_hl, glass_hl_stroke, centre, centre_on) = if dark {
            (
                rgba(1.0, 1.0, 1.0, 0.07),
                rgba(1.0, 1.0, 1.0, 0.10),
                rgba(1.0, 1.0, 1.0, 0.30),
                rgba(1.0, 1.0, 1.0, 0.60),
                rgba(1.0, 1.0, 1.0, 0.06),
                rgba(1.0, 1.0, 1.0, 0.18),
            )
        } else {
            (
                rgba(1.0, 1.0, 1.0, 0.45),
                rgba(0.0, 0.0, 0.0, 0.08),
                rgba(1.0, 1.0, 1.0, 0.97),
                rgba(0.0, 0.0, 0.0, 0.30),
                rgba(0.0, 0.0, 0.0, 0.05),
                rgba(0.0, 0.0, 0.0, 0.14),
            )
        };
        let progress = if accent {
            accent_cgcolor()
        } else if dark {
            rgba(0.96, 0.96, 0.97, 1.0)
        } else {
            rgba(0.23, 0.23, 0.24, 1.0)
        };
        let (hl_fill, hl_stroke, hl_width, hl_text) = if accent {
            let a = accent_cgcolor();
            (a.clone(), a, 0.0, NSColor::whiteColor())
        } else {
            (glass_hl, glass_hl_stroke, 2.0, NSColor::labelColor())
        };
        Self {
            sector,
            sector_stroke,
            hl_fill,
            hl_stroke,
            hl_width,
            centre,
            centre_on,
            text: NSColor::labelColor(),
            hl_text,
            progress,
        }
    }
}

/// The system accent color as an sRGB `CGColor`.
fn accent_cgcolor() -> CFRetained<CGColor> {
    let srgb = NSColorSpace::sRGBColorSpace();
    match NSColor::controlAccentColor().colorUsingColorSpace(&srgb) {
        Some(c) => CGColor::new_srgb(
            c.redComponent(),
            c.greenComponent(),
            c.blueComponent(),
            c.alphaComponent(),
        ),
        None => CGColor::new_srgb(0.0, 0.38, 0.87, 1.0),
    }
}

// ---------------------------------------------------------------------------------------
// Helpers

enum Weight {
    Regular,
    Medium,
    Semibold,
}

fn font_weight(w: Weight) -> f64 {
    // SAFETY: the font weight constants are immutable values exported by AppKit.
    unsafe {
        match w {
            Weight::Regular => NSFontWeightRegular,
            Weight::Medium => NSFontWeightMedium,
            Weight::Semibold => NSFontWeightSemibold,
        }
    }
}

fn symbol_image(name: &str, size: f64) -> Option<Retained<NSImage>> {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        None,
    )
    .or_else(|| {
        NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str("doc"),
            None,
        )
    })?;
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(
        size,
        font_weight(Weight::Regular),
    );
    image.imageWithSymbolConfiguration(&config)
}

fn symbol_view(name: &str, size: f64, mtm: MainThreadMarker) -> Retained<NSImageView> {
    let view = NSImageView::initWithFrame(NSImageView::alloc(mtm), NSRect::ZERO);
    set_symbol(&view, name, size);
    view
}

fn set_symbol(view: &NSImageView, name: &str, size: f64) {
    view.setImage(symbol_image(name, size).as_deref());
}

fn category_symbol(c: Category) -> &'static str {
    match c {
        Category::Images => "photo",
        Category::AudioVideo => "film",
        Category::Documents => "doc.text",
        Category::Utilities => "wrench.and.screwdriver",
    }
}

/// Category names until Fluent lands in Phase 4.
fn category_name(c: Category, italian: bool) -> &'static str {
    match (c, italian) {
        (Category::Images, false) => "Images",
        (Category::Images, true) => "Immagini",
        (Category::AudioVideo, false) => "Audio & Video",
        (Category::AudioVideo, true) => "Audio e video",
        (Category::Documents, false) => "Documents",
        (Category::Documents, true) => "Documenti",
        (Category::Utilities, false) => "Utilities",
        (Category::Utilities, true) => "Utilità",
    }
}

/// A ring sector between clock angles `start..end` (degrees clockwise from 12 o'clock),
/// in a non-flipped (y up) coordinate system.
fn sector_path(
    c: NSPoint,
    inner: f64,
    outer: f64,
    start: f64,
    end: f64,
) -> CFRetained<CGMutablePath> {
    // Clock angle → math angle (counter-clockwise from 3 o'clock), radians.
    let math = |clock: f64| (90.0 - clock).to_radians();
    let path = CGMutablePath::new();
    // SAFETY: a null transform pointer is explicitly allowed by every CGPath function.
    unsafe {
        // Outer arc goes clockwise (decreasing math angle), inner arc back.
        CGMutablePath::add_arc(
            Some(&path),
            std::ptr::null(),
            c.x,
            c.y,
            outer,
            math(start),
            math(end),
            true,
        );
        CGMutablePath::add_arc(
            Some(&path),
            std::ptr::null(),
            c.x,
            c.y,
            inner,
            math(end),
            math(start),
            false,
        );
    }
    CGMutablePath::close_subpath(Some(&path));
    path
}

fn without_animations(f: impl FnOnce()) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    f();
    CATransaction::commit();
}

pub fn square_around(c: NSPoint, size: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(c.x - size / 2.0, c.y - size / 2.0),
        NSSize::new(size, size),
    )
}

pub fn centre_of(r: NSRect) -> NSPoint {
    NSPoint::new(
        r.origin.x + r.size.width / 2.0,
        r.origin.y + r.size.height / 2.0,
    )
}

fn to_rect(r: NSRect) -> geometry::Rect {
    geometry::Rect {
        x: r.origin.x,
        y: r.origin.y,
        w: r.size.width,
        h: r.size.height,
    }
}

// ---------------------------------------------------------------------------------------
// Main-thread access for job events

thread_local! {
    static VIEW: RefCell<Option<Retained<RingView>>> = const { RefCell::new(None) };
}

/// Registers the view that receives job events (main thread).
pub fn register(view: &RingView) {
    VIEW.with(|v| *v.borrow_mut() = Some(view.retain()));
}

fn with_view(f: impl FnOnce(&RingView)) {
    VIEW.with(|v| {
        if let Some(view) = v.borrow().as_ref() {
            f(view);
        }
    });
}

/// Called on the main thread by the job worker.
fn on_job_event(event: Event) {
    with_view(|v| v.job_event(event));
}

struct Summary {
    ok: usize,
    failed: usize,
    text: String,
}

/// Notification text (English/Italian until Fluent lands in Phase 4).
fn summarize(label: &str, results: &[FileResult], italian: bool) -> Summary {
    let ok: Vec<&PathBuf> = results
        .iter()
        .filter_map(|r| match r {
            FileResult::Done { output, .. } => Some(output),
            _ => None,
        })
        .collect();
    let fell_back = results.iter().any(|r| {
        matches!(
            r,
            FileResult::Done {
                fell_back: true,
                ..
            }
        )
    });
    let failures: Vec<String> = results
        .iter()
        .filter_map(|r| match r {
            FileResult::Failed { input, error } => Some(format!(
                "{}: {error}",
                input
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )),
            _ => None,
        })
        .collect();
    let cancelled = results
        .iter()
        .filter(|r| matches!(r, FileResult::Cancelled))
        .count();
    let name = |p: &PathBuf| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };

    let mut parts = Vec::new();
    match (ok.len(), italian) {
        (0, _) => {}
        (1, false) => parts.push(format!("{label}: saved {}", name(ok[0]))),
        (1, true) => parts.push(format!("{label}: salvato {}", name(ok[0]))),
        (n, false) => parts.push(format!("{label}: {n} files converted")),
        (n, true) => parts.push(format!("{label}: {n} file convertiti")),
    }
    if fell_back {
        parts.push(if italian {
            "La cartella originale non è scrivibile: salvato in Download.".to_owned()
        } else {
            "The original folder is not writable: saved to Downloads.".to_owned()
        });
    }
    match (failures.len(), italian) {
        (0, _) => {}
        (1, false) => parts.push(format!("Failed: {}", failures[0])),
        (1, true) => parts.push(format!("Non riuscito: {}", failures[0])),
        (n, false) => parts.push(format!("{n} files failed (see the log)")),
        (n, true) => parts.push(format!("{n} file non riusciti (vedi il log)")),
    }
    if cancelled > 0 {
        parts.push(if italian {
            "Annullato.".to_owned()
        } else {
            "Cancelled.".to_owned()
        });
    }
    Summary {
        ok: ok.len(),
        failed: failures.len(),
        text: parts.join(" "),
    }
}
