//! Radial menu model and drag interaction state machine.
//!
//! Platform-independent so the behaviour (hover delay, category rings, dwell, drop result)
//! is unit-tested; the app only feeds it hit-test results and timestamps and renders the
//! state it reports.

use std::time::Duration;

use crate::conversion::Category;
use crate::geometry::{Hit, MAX_SECTORS};

/// Delay between the drag entering the collapsed widget and the ring expanding.
pub const HOVER_DELAY: Duration = Duration::from_millis(150);
/// How long the cursor must rest on a category (or on the centre of a category ring) to
/// open it (or go back).
pub const DWELL: Duration = Duration::from_millis(350);
/// How long the cursor must rest on a conversion with variants to open its variants ring.
/// Longer than [`DWELL`] so that pausing before a drop rarely opens it by accident.
pub const VARIANT_DWELL: Duration = Duration::from_millis(600);

/// One sector of a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// A conversion: index into the list of matches the menu was built from.
    Conversion(usize),
    /// A category that opens a second ring with this many conversions.
    Category { category: Category, count: usize },
    /// A variant of a conversion (index into the matches, index into its variants).
    Variant { conversion: usize, variant: usize },
}

/// Which ring is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Root,
    Category(Category),
    /// The variants of a conversion. `parent` is the category ring it was opened from
    /// (`None` for the root); the default variant is drawn in sector `anchor`, in the
    /// direction of the conversion's sector, so resting and dropping keeps the default.
    Variants {
        conversion: usize,
        parent: Option<Category>,
        anchor: usize,
    },
}

/// The rings available for a set of matching conversions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    /// Non-empty categories in display order, each with its match indices.
    groups: Vec<(Category, Vec<usize>)>,
    total: usize,
    /// Per match: (number of variants, index of the default variant).
    variants: Vec<(usize, usize)>,
}

impl Menu {
    /// Builds the menu from the categories of the matching conversions (in match order).
    pub fn new(categories: &[Category]) -> Self {
        Self::with_variants(categories, &[])
    }

    /// Like [`Menu::new`], with `(variant count, default variant)` for each match
    /// (missing entries mean no variants).
    pub fn with_variants(categories: &[Category], variants: &[(usize, usize)]) -> Self {
        let mut groups: Vec<(Category, Vec<usize>)> =
            Category::ALL.iter().map(|c| (*c, Vec::new())).collect();
        for (i, c) in categories.iter().enumerate() {
            if let Some((_, v)) = groups.iter_mut().find(|(g, _)| g == c) {
                v.push(i);
            }
        }
        groups.retain(|(_, v)| !v.is_empty());
        Self {
            groups,
            total: categories.len(),
            variants: variants.to_vec(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// Whether the root ring shows categories instead of conversions.
    pub fn is_grouped(&self) -> bool {
        self.total > MAX_SECTORS && self.groups.len() > 1
    }

    /// The sectors of a ring (at most [`MAX_SECTORS`]; extra conversions are dropped).
    pub fn entries(&self, level: Level) -> Vec<Entry> {
        let conversions = |v: &[usize]| -> Vec<Entry> {
            v.iter()
                .take(MAX_SECTORS)
                .map(|i| Entry::Conversion(*i))
                .collect()
        };
        match level {
            Level::Root if self.is_grouped() => self
                .groups
                .iter()
                .map(|(c, v)| Entry::Category {
                    category: *c,
                    count: v.len(),
                })
                .collect(),
            Level::Root => {
                let all: Vec<usize> = self.groups.iter().flat_map(|(_, v)| v.clone()).collect();
                conversions(&all)
            }
            Level::Variants {
                conversion, anchor, ..
            } => {
                let (n, default) = self.variant_info(conversion);
                (0..n)
                    .map(|j| Entry::Variant {
                        conversion,
                        variant: (default + j + n - anchor % n.max(1)) % n,
                    })
                    .collect()
            }
            Level::Category(c) => self
                .groups
                .iter()
                .find(|(g, _)| *g == c)
                .map(|(_, v)| conversions(v))
                .unwrap_or_default(),
        }
    }

    /// `(variant count, default variant)` of match `m`.
    pub fn variant_info(&self, m: usize) -> (usize, usize) {
        self.variants.get(m).copied().unwrap_or((0, 0))
    }

    /// Number of matches that cannot be shown because a ring is full.
    pub fn hidden_count(&self) -> usize {
        if self.is_grouped() {
            self.groups
                .iter()
                .map(|(_, v)| v.len().saturating_sub(MAX_SECTORS))
                .sum()
        } else {
            self.total.saturating_sub(MAX_SECTORS)
        }
    }
}

/// What the cursor is over, in menu terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hover {
    None,
    Centre,
    Sector(usize),
}

/// What the view must show after an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewState {
    pub expanded: bool,
    pub level: Level,
    /// Highlighted sector index (in [`Menu::entries`] of `level`).
    pub highlight: Option<usize>,
    /// The centre is highlighted (cancel / back).
    pub centre: bool,
}

/// Result of dropping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropOutcome {
    /// Run this conversion (index into the matches) with this variant (`None`: the
    /// default variant, or no variants).
    Run {
        conversion: usize,
        variant: Option<usize>,
    },
    /// Nothing to do (dead zone, outside, a category, or the ring was not open yet).
    Cancel,
}

/// The state of one drag over the widget.
#[derive(Debug, Clone)]
pub struct Interaction {
    menu: Menu,
    entered_at: Duration,
    expanded: bool,
    level: Level,
    hover: Hover,
    hover_since: Duration,
}

impl Interaction {
    /// Starts a drag that entered the widget at time `now`.
    pub fn new(menu: Menu, now: Duration) -> Self {
        Self {
            menu,
            entered_at: now,
            expanded: false,
            level: Level::Root,
            hover: Hover::None,
            hover_since: now,
        }
    }

    pub fn menu(&self) -> &Menu {
        &self.menu
    }

    /// The sectors currently shown.
    pub fn entries(&self) -> Vec<Entry> {
        self.menu.entries(self.level)
    }

    /// Feeds the latest hit-test (against the ring for the current entries) at time `now`.
    pub fn update(&mut self, hit: Hit, now: Duration) -> ViewState {
        if !self.expanded && now.saturating_sub(self.entered_at) >= HOVER_DELAY {
            self.expanded = true;
            self.set_hover(Hover::None, now);
        }
        if self.expanded {
            let count = self.entries().len();
            let hover = match hit {
                Hit::DeadZone => Hover::Centre,
                Hit::Sector(i) if i < count => Hover::Sector(i),
                _ => Hover::None,
            };
            if hover != self.hover {
                self.set_hover(hover, now);
            }
            let rested = now.saturating_sub(self.hover_since);
            let dwelled = rested >= DWELL;
            match (self.level, self.hover) {
                (Level::Root | Level::Category(_), Hover::Sector(i)) => {
                    let entries = self.entries();
                    match entries.get(i).copied() {
                        Some(Entry::Category { category, .. }) if dwelled => {
                            self.level = Level::Category(category);
                            self.set_hover(Hover::None, now);
                        }
                        Some(Entry::Conversion(m)) if rested >= VARIANT_DWELL => {
                            let (n, _) = self.menu.variant_info(m);
                            if n > 0 {
                                // Sector i of P is at i·360/P degrees; the nearest of n.
                                let p = entries.len();
                                let anchor = ((i * n * 2 + p) / (2 * p)) % n;
                                let parent = match self.level {
                                    Level::Category(c) => Some(c),
                                    _ => None,
                                };
                                self.level = Level::Variants {
                                    conversion: m,
                                    parent,
                                    anchor,
                                };
                                self.set_hover(Hover::None, now);
                            }
                        }
                        _ => {}
                    }
                }
                (Level::Category(_), Hover::Centre) if dwelled && self.menu.is_grouped() => {
                    self.level = Level::Root;
                    self.set_hover(Hover::None, now);
                }
                (Level::Variants { parent, .. }, Hover::Centre) if dwelled => {
                    self.level = parent.map_or(Level::Root, Level::Category);
                    self.set_hover(Hover::None, now);
                }
                _ => {}
            }
        }
        self.view()
    }

    fn set_hover(&mut self, hover: Hover, now: Duration) {
        self.hover = hover;
        self.hover_since = now;
    }

    /// The current view state.
    pub fn view(&self) -> ViewState {
        ViewState {
            expanded: self.expanded,
            level: self.level,
            highlight: match self.hover {
                Hover::Sector(i) if self.expanded => Some(i),
                _ => None,
            },
            centre: self.expanded && self.hover == Hover::Centre,
        }
    }

    /// The drop happened at `hit`.
    pub fn drop_at(&self, hit: Hit) -> DropOutcome {
        if !self.expanded {
            return DropOutcome::Cancel;
        }
        match hit {
            Hit::Sector(i) => match self.entries().get(i) {
                Some(Entry::Conversion(m)) => DropOutcome::Run {
                    conversion: *m,
                    variant: None,
                },
                Some(Entry::Variant {
                    conversion,
                    variant,
                }) => DropOutcome::Run {
                    conversion: *conversion,
                    variant: Some(*variant),
                },
                _ => DropOutcome::Cancel,
            },
            Hit::DeadZone | Hit::Outside => DropOutcome::Cancel,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Category::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn small_menus_are_flat_in_category_order() {
        let m = Menu::new(&[Utilities, Images, Images]);
        assert!(!m.is_grouped());
        assert_eq!(
            m.entries(Level::Root),
            vec![
                Entry::Conversion(1),
                Entry::Conversion(2),
                Entry::Conversion(0)
            ]
        );
        assert_eq!(m.hidden_count(), 0);
        assert!(Menu::new(&[]).is_empty());
    }

    #[test]
    fn large_menus_group_by_category() {
        let mut cats = vec![Images; 6];
        cats.extend([Documents, Documents, Utilities]);
        let m = Menu::new(&cats);
        assert!(m.is_grouped());
        assert_eq!(
            m.entries(Level::Root),
            vec![
                Entry::Category {
                    category: Images,
                    count: 6
                },
                Entry::Category {
                    category: Documents,
                    count: 2
                },
                Entry::Category {
                    category: Utilities,
                    count: 1
                },
            ]
        );
        assert_eq!(
            m.entries(Level::Category(Documents)),
            vec![Entry::Conversion(6), Entry::Conversion(7)]
        );
        assert!(m.entries(Level::Category(AudioVideo)).is_empty());
    }

    #[test]
    fn single_category_overflow_is_truncated_not_grouped() {
        let m = Menu::new(&[Images; 11]);
        assert!(!m.is_grouped());
        assert_eq!(m.entries(Level::Root).len(), MAX_SECTORS);
        assert_eq!(m.hidden_count(), 3);
    }

    #[test]
    fn hover_delay_before_expanding() {
        let mut it = Interaction::new(Menu::new(&[Images, Images]), ms(1000));
        assert!(!it.update(Hit::Sector(0), ms(1100)).expanded);
        assert_eq!(it.drop_at(Hit::Sector(0)), DropOutcome::Cancel);
        let v = it.update(Hit::Sector(1), ms(1150));
        assert!(v.expanded);
        assert_eq!(v.highlight, Some(1));
        assert_eq!(
            it.drop_at(Hit::Sector(1)),
            DropOutcome::Run {
                conversion: 1,
                variant: None
            }
        );
    }

    #[test]
    fn centre_and_outside() {
        let mut it = Interaction::new(Menu::new(&[Images]), ms(0));
        let v = it.update(Hit::DeadZone, ms(200));
        assert!(v.centre && v.highlight.is_none());
        assert_eq!(it.drop_at(Hit::DeadZone), DropOutcome::Cancel);
        let v = it.update(Hit::Outside, ms(250));
        assert!(!v.centre && v.highlight.is_none());
        // A sector index beyond the entries is ignored.
        assert_eq!(it.update(Hit::Sector(5), ms(300)).highlight, None);
        assert_eq!(it.drop_at(Hit::Sector(5)), DropOutcome::Cancel);
    }

    #[test]
    fn dwell_opens_category_and_centre_goes_back() {
        let mut cats = vec![Images; 7];
        cats.extend([Documents, Documents]);
        let mut it = Interaction::new(Menu::new(&cats), ms(0));
        it.update(Hit::Sector(1), ms(200)); // Documents, hover starts
        assert_eq!(
            it.drop_at(Hit::Sector(1)),
            DropOutcome::Cancel,
            "categories don't run"
        );
        assert_eq!(it.update(Hit::Sector(1), ms(500)).level, Level::Root);
        let v = it.update(Hit::Sector(1), ms(560));
        assert_eq!(v.level, Level::Category(Documents));
        assert_eq!(v.highlight, None, "highlight resets in the new ring");
        assert_eq!(
            it.entries(),
            vec![Entry::Conversion(7), Entry::Conversion(8)]
        );

        // Still on sector 1 (now a conversion): highlighted, and dropping runs it.
        assert_eq!(it.update(Hit::Sector(1), ms(600)).highlight, Some(1));
        assert_eq!(
            it.drop_at(Hit::Sector(1)),
            DropOutcome::Run {
                conversion: 8,
                variant: None
            }
        );

        // Rest on the centre to go back.
        it.update(Hit::DeadZone, ms(700));
        assert_eq!(
            it.update(Hit::DeadZone, ms(1000)).level,
            Level::Category(Documents)
        );
        assert_eq!(it.update(Hit::DeadZone, ms(1060)).level, Level::Root);
    }

    #[test]
    fn moving_resets_dwell() {
        let mut cats = vec![Images; 7];
        cats.extend([Documents, Utilities]);
        let mut it = Interaction::new(Menu::new(&cats), ms(0));
        it.update(Hit::Sector(0), ms(200));
        it.update(Hit::Sector(1), ms(400));
        assert_eq!(it.update(Hit::Sector(1), ms(700)).level, Level::Root);
        assert_eq!(
            it.update(Hit::Sector(1), ms(760)).level,
            Level::Category(Documents)
        );
    }

    #[test]
    fn variants_ring_opens_on_a_longer_rest_and_keeps_the_default_under_the_cursor() {
        // Four image conversions; the one at index 2 has 3 variants, default = 1.
        let m = Menu::with_variants(&[Images, Images, Images, Images], &[(0, 0), (0, 0), (3, 1)]);
        let mut it = Interaction::new(m, ms(0));
        it.update(Hit::Outside, HOVER_DELAY);
        // Sector 2 of 4 points down (180°).
        assert_eq!(it.update(Hit::Sector(2), ms(200)).level, Level::Root);
        // After DWELL (categories) nothing happens: conversions need VARIANT_DWELL.
        assert_eq!(it.update(Hit::Sector(2), ms(600)).level, Level::Root);
        // A quick drop runs the default variant.
        assert_eq!(
            it.drop_at(Hit::Sector(2)),
            DropOutcome::Run {
                conversion: 2,
                variant: None
            }
        );
        let v = it.update(Hit::Sector(2), ms(820));
        let Level::Variants {
            conversion,
            parent,
            anchor,
        } = v.level
        else {
            panic!("variants ring expected, got {:?}", v.level);
        };
        assert_eq!((conversion, parent), (2, None));
        // 180° with 3 sectors (0°, 120°, 240°) → nearest is sector 2 (240°) or 1 (120°);
        // round(2*3/4) = 2.
        assert_eq!(anchor, 2);
        let entries = it.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[anchor],
            Entry::Variant {
                conversion: 2,
                variant: 1
            }
        );
        // Every variant appears exactly once.
        let mut seen: Vec<usize> = entries
            .iter()
            .map(|e| match e {
                Entry::Variant { variant, .. } => *variant,
                _ => panic!("variant entries only"),
            })
            .collect();
        seen.sort();
        assert_eq!(seen, vec![0, 1, 2]);
        let Entry::Variant { variant: first, .. } = entries[0] else {
            unreachable!()
        };
        assert_eq!(
            it.drop_at(Hit::Sector(0)),
            DropOutcome::Run {
                conversion: 2,
                variant: Some(first)
            }
        );
        // Resting on the centre goes back to the ring it came from.
        it.update(Hit::DeadZone, ms(900));
        assert_eq!(it.update(Hit::DeadZone, ms(1300)).level, Level::Root);
        // Conversions without variants never open a ring.
        it.update(Hit::Sector(0), ms(1400));
        assert_eq!(it.update(Hit::Sector(0), ms(2500)).level, Level::Root);
    }

    #[test]
    fn variants_ring_from_a_category_goes_back_to_the_category() {
        let mut categories = vec![Images; 9];
        categories.push(Documents);
        let mut variants = vec![(0, 0); 10];
        variants[9] = (2, 0);
        let m = Menu::with_variants(&categories, &variants);
        assert!(m.is_grouped());
        let mut it = Interaction::new(m, ms(0));
        it.update(Hit::Outside, HOVER_DELAY);
        // Root: [Images, Documents]; rest on Documents.
        it.update(Hit::Sector(1), ms(200));
        assert_eq!(
            it.update(Hit::Sector(1), ms(600)).level,
            Level::Category(Documents)
        );
        // The Documents ring has one conversion (index 9) with 2 variants.
        it.update(Hit::Sector(0), ms(700));
        let v = it.update(Hit::Sector(0), ms(1400));
        assert!(matches!(
            v.level,
            Level::Variants {
                conversion: 9,
                parent: Some(Documents),
                ..
            }
        ));
        it.update(Hit::DeadZone, ms(1500));
        assert_eq!(
            it.update(Hit::DeadZone, ms(1900)).level,
            Level::Category(Documents)
        );
    }
}
