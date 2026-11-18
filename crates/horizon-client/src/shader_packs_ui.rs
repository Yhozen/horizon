//! What the three shader pack pages show (Shader Packs, Shader Pack Settings, Shader Pack Report)
//! and the events they send to the game. The game fills [`ShaderPacksView`] every frame while one
//! of the pages shows (`OptionsUi::shader_packs`); `options_ui` lays the pages out from the pure
//! functions here with vanilla's widgets.

use crate::text::{self, Text};
use horizon_glsl_pack::report::PackStatus;
use std::collections::BTreeSet;
use std::sync::Arc;

pub use model::{Entry, OptionDef, OptionModel, Screen};

/// The pack options model the Settings page is built from. A local stand-in for
/// `horizon_glsl_pack::screens` until that module lands; the page reads it only through the
/// accessors below, so the switch touches this module alone.
pub mod model {
    use std::collections::BTreeMap;

    /// Every option of a pack, its screens, profiles and language strings.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct OptionModel {
        pub options: Vec<OptionDef>,
        /// `screen=`; `None` when the pack has none (the page lists every option).
        pub root: Option<Screen>,
        /// `screen.NAME=`, by name.
        pub screens: BTreeMap<String, Screen>,
        /// `profile.NAME=`, in file order, each resolved to option values.
        pub profiles: Vec<(String, Vec<(String, String)>)>,
        /// `lang/en_us.lang` (`option.X`, `value.X.v`, `screen.X`...).
        pub lang: BTreeMap<String, String>,
    }
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct OptionDef {
        pub name: String,
        pub default: String,
        /// The allowed values in order (a boolean has `true` and `false` or none).
        pub values: Vec<String>,
        /// Listed in `sliders=`.
        pub slider: bool,
        /// A `#define NAME` switch rather than a value.
        pub boolean: bool,
    }
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Screen {
        pub entries: Vec<Entry>,
        /// `screen.NAME.columns` (0: the default, 2).
        pub columns: usize,
    }
    #[derive(Clone, Debug, PartialEq)]
    pub enum Entry {
        Option(String),
        Screen(String),
        /// `<profile>`.
        Profile,
        /// `<empty>`.
        Empty,
        /// `*`: every option no screen lists.
        Rest,
    }
}

// The only places that read the model's fields.
fn find_option<'a>(m: &'a OptionModel, name: &str) -> Option<&'a OptionDef> {
    m.options.iter().find(|o| o.name == name)
}
fn screen_of<'a>(m: &'a OptionModel, name: Option<&str>) -> Option<&'a Screen> {
    match name {
        None => m.root.as_ref(),
        Some(name) => m.screens.get(name),
    }
}
fn all_screens(m: &OptionModel) -> impl Iterator<Item = (Option<&str>, &Screen)> {
    m.root
        .iter()
        .map(|s| (None, s))
        .chain(m.screens.iter().map(|(n, s)| (Some(n.as_str()), s)))
}
fn lang<'a>(m: &'a OptionModel, key: &str) -> Option<&'a str> {
    m.lang.get(key).map(String::as_str)
}
fn profiles(m: &OptionModel) -> &[(String, Vec<(String, String)>)] {
    &m.profiles
}

/// What the game shows on the three pages; refreshed every frame while one shows.
#[derive(Clone, Debug, Default)]
pub struct ShaderPacksView {
    /// The "Shaders: ON/OFF" row.
    pub enabled: bool,
    /// Sorted by file name.
    pub packs: Vec<PackRow>,
    /// File name of the chosen pack (may be unapplied).
    pub selected: Option<String>,
    /// File name of the pack running now.
    pub applied: Option<String>,
    /// Compiling: programs done / total.
    pub progress: Option<(usize, usize)>,
    /// Browser: "Add Shader Pack…" instead of "Open Shader Pack Folder…".
    pub web: bool,
    /// The selected pack's report, one line each (empty: not tried).
    pub report: Vec<String>,
    /// The selected pack's options (`None`: no pack selected).
    pub settings: Option<SettingsView>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PackRow {
    pub file: String,
    /// May hold `§` codes.
    pub title: String,
    pub status: Option<PackStatus>,
}
#[derive(Clone, Debug, Default)]
pub struct SettingsView {
    pub model: Arc<OptionModel>,
    /// Current non-default values.
    pub values: Vec<(String, String)>,
    /// Sub-screen shown (`None` = root).
    pub screen: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShaderPackEvent {
    SetEnabled(bool),
    Select(String),
    Apply,
    OpenFolder,
    /// The web picker; emitted from the click itself (the browser's user activation).
    AddPack,
    Delete(String),
    CopyReport,
    OpenScreen(Option<String>),
    SetOption {
        name: String,
        value: String,
    },
    ApplyProfile(String),
    ResetOptions,
}

impl ShaderPacksView {
    /// The pack the game should run, as the page last set it.
    pub fn wanted(&self) -> Option<&str> {
        self.selected.as_deref().filter(|_| self.enabled)
    }
    /// Whether leaving the page has a change to apply.
    pub fn unapplied(&self) -> bool {
        self.wanted() != self.applied.as_deref()
    }
}

/// A pack's row: its title (with its `§` codes) and its status word in colour.
pub fn pack_label(row: &PackRow) -> Text {
    let title = Text::legacy(&row.title);
    match row.status {
        Some(status) => title
            .append(" ")
            .append(Text::colored(status.label(), status_color(status))),
        None => title,
    }
}
/// Grey for refused, green, yellow, red.
pub fn status_color(status: PackStatus) -> u32 {
    match status {
        PackStatus::Works => text::GREEN,
        PackStatus::Partial => text::YELLOW,
        PackStatus::Refused => text::GRAY,
        PackStatus::Failed => text::RED,
    }
}
/// "Compiling 12/41".
pub fn progress_line(progress: (usize, usize)) -> String {
    format!("Compiling {}/{}", progress.0, progress.1)
}

/// The next (or previous, `back`) of `values` after `current`; the first when `current` is not
/// one of them.
pub fn cycle(values: &[String], current: &str, back: bool) -> String {
    let Some(i) = values.iter().position(|v| v == current) else {
        return values
            .first()
            .cloned()
            .unwrap_or_else(|| current.to_string());
    };
    let n = values.len();
    values[if back { (i + n - 1) % n } else { (i + 1) % n }].clone()
}
/// Where a slider's handle sits for `current`, 0 to 1.
pub fn slider_fraction(values: &[String], current: &str) -> f32 {
    match values.iter().position(|v| v == current) {
        Some(i) if values.len() > 1 => i as f32 / (values.len() - 1) as f32,
        _ => 0.0,
    }
}
/// The value nearest a slider position.
pub fn slider_pick(values: &[String], fraction: f32) -> Option<String> {
    let last = values.len().checked_sub(1)?;
    let i = (fraction.clamp(0.0, 1.0) * last as f32).round() as usize;
    values.get(i.min(last)).cloned()
}

/// The values an option cycles through (a switch: on, off).
fn option_values(o: &OptionDef) -> Vec<String> {
    if o.boolean && o.values.is_empty() {
        vec!["true".into(), "false".into()]
    } else {
        o.values.clone()
    }
}
/// The values a slider of `name` spans.
pub fn option_choices(view: &SettingsView, name: &str) -> Vec<String> {
    find_option(&view.model, name).map_or_else(Vec::new, option_values)
}
/// An option's hover text (`option.X.comment`), without its `§` codes.
pub fn option_tooltip(view: &SettingsView, name: &str) -> Option<String> {
    lang(&view.model, &format!("option.{name}.comment")).map(|t| Text::legacy(t).plain())
}
/// An option's current value.
pub fn value_of<'a>(view: &'a SettingsView, name: &'a str) -> Option<&'a str> {
    view.values
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
        .or_else(|| find_option(&view.model, name).map(|o| o.default.as_str()))
}
/// Sets an option in the view at once (the game confirms on its next refresh): a value equal to
/// the default is dropped from `values`.
pub fn set_value(view: &mut SettingsView, name: &str, value: &str) {
    let default = find_option(&view.model, name).map(|o| o.default.clone());
    view.values.retain(|(n, _)| n != name);
    if default.as_deref() != Some(value) {
        view.values.push((name.to_string(), value.to_string()));
    }
}
/// The value after a click on an option (right-click goes back).
pub fn next_value(view: &SettingsView, name: &str, back: bool) -> Option<String> {
    let o = find_option(&view.model, name)?;
    Some(cycle(&option_values(o), value_of(view, name)?, back))
}
/// The first profile whose values all match the current ones.
pub fn current_profile(view: &SettingsView) -> Option<&str> {
    profiles(&view.model)
        .iter()
        .find(|(_, values)| {
            values
                .iter()
                .all(|(n, v)| value_of(view, n) == Some(v.as_str()))
        })
        .map(|(name, _)| name.as_str())
}
/// The profile a click on the profile button applies.
pub fn next_profile(view: &SettingsView, back: bool) -> Option<String> {
    let names: Vec<String> = profiles(&view.model)
        .iter()
        .map(|(n, _)| n.clone())
        .collect();
    if names.is_empty() {
        return None;
    }
    match current_profile(view) {
        Some(current) => Some(cycle(&names, current, back)),
        None => names.first().cloned(),
    }
}
/// Applies a profile's values to the view at once.
pub fn apply_profile(view: &mut SettingsView, profile: &str) {
    let model = view.model.clone();
    if let Some((_, values)) = profiles(&model).iter().find(|(n, _)| n == profile) {
        for (name, value) in values {
            set_value(view, name, value);
        }
    }
}

/// One cell of the Settings page.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Option {
        name: String,
        label: Text,
        tooltip: Option<String>,
        /// `Some(position)` for a slider.
        slider: Option<f32>,
    },
    Screen {
        name: String,
        label: Text,
    },
    Profile {
        label: Text,
    },
    Empty,
}
/// The Settings page's cells and their columns.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsPage {
    pub columns: usize,
    pub items: Vec<Item>,
}

/// `option.X`, else the option's name.
fn option_caption(m: &OptionModel, name: &str) -> Text {
    Text::legacy(lang(m, &format!("option.{name}")).unwrap_or(name))
}
/// "Caption: value" as the pack's language file words it (value names, prefix, suffix); a
/// switch says ON in green or OFF in red.
pub fn option_label(m: &OptionModel, o: &OptionDef, value: &str) -> Text {
    let caption = option_caption(m, &o.name).append(": ");
    if o.boolean {
        let on = value == "true";
        return caption.append(Text::colored(
            if on { "ON" } else { "OFF" },
            if on { text::GREEN } else { text::RED },
        ));
    }
    let shown = lang(m, &format!("value.{}.{value}", o.name)).unwrap_or(value);
    let prefix = lang(m, &format!("prefix.{}", o.name)).unwrap_or("");
    let suffix = lang(m, &format!("suffix.{}", o.name)).unwrap_or("");
    caption.append(Text::legacy(&format!("{prefix}{shown}{suffix}")))
}
/// `screen.X`, else the screen's name, then "...".
pub fn screen_label(m: &OptionModel, name: &str) -> Text {
    Text::legacy(lang(m, &format!("screen.{name}")).unwrap_or(name)).append("...")
}
/// "Profile: NAME" (`profile.X`), or "Custom" when no profile matches.
pub fn profile_label(view: &SettingsView) -> Text {
    let name = match current_profile(view) {
        Some(p) => lang(&view.model, &format!("profile.{p}"))
            .unwrap_or(p)
            .to_string(),
        None => "Custom".to_string(),
    };
    Text::literal("Profile: ").append(Text::legacy(&name))
}
/// The title of the screen shown: "Shader Pack Settings" at the root, else the screen's label.
pub fn settings_title(view: &SettingsView) -> String {
    match &view.screen {
        None => "Shader Pack Settings".to_string(),
        Some(name) => lang(&view.model, &format!("screen.{name}"))
            .map(|s| Text::legacy(s).plain())
            .unwrap_or_else(|| name.clone()),
    }
}
/// The screen holding a link to `screen` (`None`: the root); the root has no parent.
pub fn parent_screen(m: &OptionModel, screen: &str) -> Option<String> {
    all_screens(m)
        .find(|(_, s)| {
            s.entries
                .iter()
                .any(|e| matches!(e, Entry::Screen(n) if n == screen))
        })
        .and_then(|(name, _)| name.map(str::to_string))
}

fn option_item(view: &SettingsView, name: &str) -> Option<Item> {
    let o = find_option(&view.model, name)?;
    let value = value_of(view, name).unwrap_or(&o.default);
    let values = option_values(o);
    Some(Item::Option {
        name: name.to_string(),
        label: option_label(&view.model, o, value),
        tooltip: option_tooltip(view, name),
        slider: (o.slider && !o.boolean && values.len() > 1)
            .then(|| slider_fraction(&values, value)),
    })
}

/// The cells of the screen shown: its entries in order, `*` as every option no screen lists, and
/// every option (two columns) when the pack has no `screen=`.
pub fn settings_page(view: &SettingsView) -> SettingsPage {
    let m = &*view.model;
    let Some(screen) = screen_of(m, view.screen.as_deref()) else {
        return SettingsPage {
            columns: 2,
            items: m
                .options
                .iter()
                .filter_map(|o| option_item(view, &o.name))
                .collect(),
        };
    };
    let listed: BTreeSet<&str> = all_screens(m)
        .flat_map(|(_, s)| s.entries.iter())
        .filter_map(|e| match e {
            Entry::Option(n) => Some(n.as_str()),
            _ => None,
        })
        .collect();
    let mut items = Vec::new();
    for entry in &screen.entries {
        match entry {
            Entry::Option(name) => items.extend(option_item(view, name)),
            Entry::Screen(name) => items.push(Item::Screen {
                name: name.clone(),
                label: screen_label(m, name),
            }),
            Entry::Profile => items.push(Item::Profile {
                label: profile_label(view),
            }),
            Entry::Empty => items.push(Item::Empty),
            Entry::Rest => items.extend(
                m.options
                    .iter()
                    .filter(|o| !listed.contains(o.name.as_str()))
                    .filter_map(|o| option_item(view, &o.name)),
            ),
        }
    }
    SettingsPage {
        columns: if screen.columns == 0 {
            2
        } else {
            screen.columns
        },
        items,
    }
}

/// The column count and button width for `requested` columns on a screen `width` wide: one 310
/// button, two of 150, more shrink to fit; narrower than 90 falls back to two.
pub fn columns_fit(requested: usize, width: f32) -> (usize, f32) {
    match requested {
        0 | 2 => (2, 150.0),
        1 => (1, 310.0),
        c => {
            let available = (width - 40.0).min(c as f32 * 160.0);
            let button = ((available - 10.0 * (c - 1) as f32) / c as f32).floor();
            if button < 90.0 {
                (2, 150.0)
            } else {
                (c, button)
            }
        }
    }
}

/// Greedy word wrap of `lines` to `width` by `measure`; a word longer than the width is broken
/// between characters. Empty lines stay.
pub fn wrap(lines: &[String], width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in lines {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if measure(&candidate) <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            for c in word.chars() {
                let mut next = line.clone();
                next.push(c);
                if measure(&next) > width && !line.is_empty() {
                    out.push(std::mem::replace(&mut line, c.to_string()));
                } else {
                    line = next;
                }
            }
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> String {
        v.to_string()
    }
    fn model() -> OptionModel {
        let opt =
            |name: &str, default: &str, values: &[&str], slider: bool, boolean: bool| OptionDef {
                name: s(name),
                default: s(default),
                values: values.iter().map(|v| s(v)).collect(),
                slider,
                boolean,
            };
        OptionModel {
            options: vec![
                opt("SHADOWS", "true", &[], false, true),
                opt("SUN", "1.0", &["0.5", "1.0", "2.0"], true, false),
                opt("QUALITY", "1", &["0", "1", "2"], false, false),
                opt("LOOSE", "a", &["a", "b"], false, false),
            ],
            root: Some(Screen {
                entries: vec![
                    Entry::Profile,
                    Entry::Empty,
                    Entry::Screen(s("LIGHT")),
                    Entry::Option(s("QUALITY")),
                    Entry::Rest,
                ],
                columns: 0,
            }),
            screens: [(
                s("LIGHT"),
                Screen {
                    entries: vec![Entry::Option(s("SHADOWS")), Entry::Option(s("SUN"))],
                    columns: 1,
                },
            )]
            .into(),
            profiles: vec![
                (
                    s("LOW"),
                    vec![(s("QUALITY"), s("0")), (s("SHADOWS"), s("false"))],
                ),
                (
                    s("HIGH"),
                    vec![(s("QUALITY"), s("2")), (s("SHADOWS"), s("true"))],
                ),
            ],
            lang: [
                (s("option.SUN"), s("Sun")),
                (s("value.QUALITY.2"), s("§aHigh")),
                (s("suffix.SUN"), s("x")),
                (s("screen.LIGHT"), s("Lighting")),
                (s("profile.LOW"), s("Low")),
                (s("option.QUALITY.comment"), s("How good. §eSlow.")),
            ]
            .into(),
        }
    }
    fn view() -> SettingsView {
        SettingsView {
            model: Arc::new(model()),
            values: Vec::new(),
            screen: None,
        }
    }

    #[test]
    fn values_cycle_both_ways_and_wrap() {
        let v: Vec<String> = ["0", "1", "2"].map(s).to_vec();
        assert_eq!(cycle(&v, "1", false), "2");
        assert_eq!(cycle(&v, "2", false), "0");
        assert_eq!(cycle(&v, "0", true), "2");
        assert_eq!(cycle(&v, "7", false), "0");
        let mut view = view();
        assert_eq!(
            next_value(&view, "SHADOWS", false).as_deref(),
            Some("false")
        );
        set_value(&mut view, "SHADOWS", "false");
        assert_eq!(view.values, vec![(s("SHADOWS"), s("false"))]);
        assert_eq!(next_value(&view, "SHADOWS", true).as_deref(), Some("true"));
        // Back to the default leaves no entry.
        set_value(&mut view, "SHADOWS", "true");
        assert!(view.values.is_empty());
    }

    #[test]
    fn sliders_map_positions_to_values() {
        let v: Vec<String> = ["0.5", "1.0", "2.0"].map(s).to_vec();
        assert_eq!(slider_fraction(&v, "1.0"), 0.5);
        assert_eq!(slider_pick(&v, 0.8).as_deref(), Some("2.0"));
        assert_eq!(slider_pick(&v, -1.0).as_deref(), Some("0.5"));
        assert_eq!(slider_pick(&[], 0.5), None);
    }

    #[test]
    fn root_screen_rows_follow_the_pack_with_rest_and_labels() {
        let page = settings_page(&view());
        assert_eq!(page.columns, 2);
        let plain: Vec<String> = page
            .items
            .iter()
            .map(|i| match i {
                Item::Option { label, .. }
                | Item::Screen { label, .. }
                | Item::Profile { label } => label.plain(),
                Item::Empty => s("<empty>"),
            })
            .collect();
        assert_eq!(
            plain,
            [
                "Profile: Custom",
                "<empty>",
                "Lighting...",
                "QUALITY: 1",
                "LOOSE: a"
            ]
        );
        let Item::Option {
            tooltip, slider, ..
        } = &page.items[3]
        else {
            panic!()
        };
        assert_eq!(tooltip.as_deref(), Some("How good. Slow."));
        assert_eq!(*slider, None);
    }

    #[test]
    fn sub_screens_switches_sliders_and_parents() {
        let mut view = view();
        view.screen = Some(s("LIGHT"));
        assert_eq!(settings_title(&view), "Lighting");
        let page = settings_page(&view);
        assert_eq!(page.columns, 1);
        let Item::Option { label, .. } = &page.items[0] else {
            panic!()
        };
        assert_eq!(label.plain(), "SHADOWS: ON");
        assert_eq!(label.spans.last().unwrap().style.color, Some(text::GREEN));
        let Item::Option { label, slider, .. } = &page.items[1] else {
            panic!()
        };
        assert_eq!(label.plain(), "Sun: 1.0x");
        assert_eq!(*slider, Some(0.5));
        assert_eq!(parent_screen(&view.model, "LIGHT"), None);
    }

    #[test]
    fn value_names_and_profiles() {
        let mut view = view();
        set_value(&mut view, "QUALITY", "2");
        let m = view.model.clone();
        assert_eq!(
            option_label(&m, &m.options[2], "2").plain(),
            "QUALITY: High"
        );
        assert_eq!(current_profile(&view), Some("HIGH"));
        assert_eq!(next_profile(&view, false).as_deref(), Some("LOW"));
        apply_profile(&mut view, "LOW");
        assert_eq!(profile_label(&view).plain(), "Profile: Low");
        assert_eq!(value_of(&view, "SHADOWS"), Some("false"));
    }

    #[test]
    fn a_pack_without_screens_lists_every_option() {
        let mut m = model();
        m.root = None;
        let view = SettingsView {
            model: Arc::new(m),
            ..view()
        };
        let page = settings_page(&view);
        assert_eq!((page.columns, page.items.len()), (2, 4));
    }

    #[test]
    fn columns_fall_back_to_two_when_too_wide() {
        assert_eq!(columns_fit(0, 427.0), (2, 150.0));
        assert_eq!(columns_fit(1, 427.0), (1, 310.0));
        assert_eq!(columns_fit(3, 427.0), (3, 122.0));
        assert_eq!(columns_fit(4, 427.0), (2, 150.0));
        assert_eq!(columns_fit(4, 900.0).0, 4);
    }

    #[test]
    fn report_lines_wrap_by_width() {
        let measure = |t: &str| t.chars().count() as f32;
        assert_eq!(
            wrap(&[s("aa bb cc"), s(""), s("abcdefg")], 5.0, measure),
            ["aa bb", "cc", "", "abcde", "fg"]
        );
    }

    #[test]
    fn pack_rows_show_status_in_colour_and_track_unapplied_changes() {
        let row = PackRow {
            file: s("BSL.zip"),
            title: s("§bBSL"),
            status: Some(PackStatus::Partial),
        };
        let label = pack_label(&row);
        assert_eq!(label.plain(), "BSL Partial");
        assert_eq!(label.spans.last().unwrap().style.color, Some(text::YELLOW));
        let mut v = ShaderPacksView {
            enabled: true,
            selected: Some(s("BSL.zip")),
            ..Default::default()
        };
        assert!(v.unapplied());
        v.applied = Some(s("BSL.zip"));
        assert!(!v.unapplied());
        v.enabled = false;
        assert!(v.unapplied());
        assert_eq!(progress_line((12, 41)), "Compiling 12/41");
    }
}
