//! What the three shader pack pages show (Shader Packs, Shader Pack Settings, Shader Pack Report)
//! and the events they send to the game. The game fills [`ShaderPacksView`] every frame while one
//! of the pages shows (`OptionsUi::shader_packs`); `options_ui` lays the pages out from the pure
//! functions here with vanilla's widgets. The Settings page is built from the pack's own
//! [`OptionModel`] (`horizon_glsl_pack::screens`).

use crate::text::{self, Text};
use horizon_glsl_pack::report::PackStatus;
use std::sync::Arc;

pub use horizon_glsl_pack::screens::{Entry, OptionInfo, OptionModel, Screen, Values};

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

impl SettingsView {
    /// Every option's current value.
    pub fn current(&self) -> Values {
        self.model.values_from(&self.values)
    }
    /// The screen shown, as the model names it (`""`: the main screen).
    fn screen_key(&self) -> &str {
        self.screen.as_deref().unwrap_or("")
    }
    /// Replaces the values, keeping only the non-default ones.
    fn store(&mut self, values: &Values) {
        self.values = self.model.changed(values);
    }
}

/// The values a slider of `name` spans.
pub fn option_choices(view: &SettingsView, name: &str) -> Vec<String> {
    view.model
        .option(name)
        .map_or_else(Vec::new, |o| o.values.clone())
}
/// An option's hover text (`option.X.comment`), without its `§` codes.
pub fn option_tooltip(view: &SettingsView, name: &str) -> Option<String> {
    view.model
        .lang
        .option_comment(name)
        .map(|t| Text::legacy(t).plain())
}
/// An option's current value.
pub fn value_of(view: &SettingsView, name: &str) -> Option<String> {
    view.values
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.clone())
        .or_else(|| view.model.option(name).map(|o| o.default.clone()))
}
/// Sets an option in the view at once (the game confirms on its next refresh).
pub fn set_value(view: &mut SettingsView, name: &str, value: &str) {
    let mut values = view.current();
    values.insert(name.to_string(), value.to_string());
    view.store(&values);
}
/// The value after a click on an option (right-click goes back).
pub fn next_value(view: &SettingsView, name: &str, back: bool) -> Option<String> {
    view.model.next_value(name, &value_of(view, name)?, back)
}
/// The first profile whose values all match the current ones.
pub fn current_profile(view: &SettingsView) -> Option<String> {
    view.model
        .current_profile(&view.current())
        .map(str::to_string)
}
/// The profile a click on the profile button applies.
pub fn next_profile(view: &SettingsView, back: bool) -> Option<String> {
    let names: Vec<String> = view.model.profiles.iter().map(|p| p.name.clone()).collect();
    if names.is_empty() {
        return None;
    }
    match current_profile(view) {
        Some(current) => Some(cycle(&names, &current, back)),
        None => names.first().cloned(),
    }
}
/// Applies a profile's values to the view at once.
pub fn apply_profile(view: &mut SettingsView, profile: &str) {
    let values = horizon_glsl_pack::screens::apply_profile(&view.model, profile, &view.current());
    view.store(&values);
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

/// "Caption: value" as the pack's language file words it (value names, prefix, suffix); a
/// switch without its own value names says ON in green or OFF in red.
pub fn option_label(m: &OptionModel, o: &OptionInfo, value: &str) -> Text {
    let caption = Text::legacy(&m.lang.option(&o.name)).append(": ");
    let shown = m.lang.value(o, value);
    let named = m.lang.get(&format!("value.{}.{value}", o.name)).is_some();
    if o.boolean && !named {
        let color = if value == "true" {
            text::GREEN
        } else {
            text::RED
        };
        return caption.append(Text::colored(shown, color));
    }
    caption.append(Text::legacy(&shown))
}
/// `screen.X`, else the screen's name, then "...".
pub fn screen_label(m: &OptionModel, name: &str) -> Text {
    Text::legacy(&m.lang.screen(name)).append("...")
}
/// "Profile: NAME" (`profile.X`), or "Custom" when no profile matches.
pub fn profile_label(view: &SettingsView) -> Text {
    let name = match current_profile(view) {
        Some(p) => view.model.lang.profile(&p),
        None => "Custom".to_string(),
    };
    Text::literal("Profile: ").append(Text::legacy(&name))
}
/// The title of the screen shown: "Shader Pack Settings" at the root, else the screen's label.
pub fn settings_title(view: &SettingsView) -> String {
    match &view.screen {
        None => "Shader Pack Settings".to_string(),
        Some(name) => Text::legacy(&view.model.lang.screen(name)).plain(),
    }
}
/// The screen holding a link to `screen` (`None`: the main screen).
pub fn parent_screen(m: &OptionModel, screen: &str) -> Option<String> {
    m.screens
        .iter()
        .find(|(_, s)| {
            s.entries
                .iter()
                .any(|e| matches!(e, Entry::Screen(n) if n == screen))
        })
        .map(|(name, _)| name.clone())
        .filter(|name| !name.is_empty())
}

fn option_item(view: &SettingsView, values: &Values, name: &str) -> Option<Item> {
    let o = view.model.option(name)?;
    let value = values.get(name).unwrap_or(&o.default);
    Some(Item::Option {
        name: name.to_string(),
        label: option_label(&view.model, o, value),
        tooltip: option_tooltip(view, name),
        slider: (o.slider && !o.boolean && o.values.len() > 1)
            .then(|| slider_fraction(&o.values, value)),
    })
}

/// The cells of the screen shown, `*` expanded by the model; every option (two columns) when the
/// model has no such screen.
pub fn settings_page(view: &SettingsView) -> SettingsPage {
    let m = &*view.model;
    let values = view.current();
    let Some(screen) = m.screens.get(view.screen_key()) else {
        return SettingsPage {
            columns: 2,
            items: m
                .options
                .iter()
                .filter_map(|o| option_item(view, &values, &o.name))
                .collect(),
        };
    };
    let items = m
        .entries(view.screen_key())
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Option(name) => option_item(view, &values, &name),
            Entry::Screen(name) => Some(Item::Screen {
                label: screen_label(m, &name),
                name,
            }),
            Entry::Profile => Some(Item::Profile {
                label: profile_label(view),
            }),
            Entry::Empty | Entry::Rest => Some(Item::Empty),
        })
        .collect();
    SettingsPage {
        columns: if screen.columns == 0 {
            2
        } else {
            screen.columns as usize
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
    use horizon_glsl_pack::options::OptionKind;
    use horizon_glsl_pack::screens::{Lang, Profile};

    fn s(v: &str) -> String {
        v.to_string()
    }
    fn opt(name: &str, default: &str, values: &[&str], slider: bool) -> OptionInfo {
        let boolean = values.is_empty();
        OptionInfo {
            name: s(name),
            kind: if boolean {
                OptionKind::Bool
            } else {
                OptionKind::Value
            },
            default: s(default),
            values: if boolean {
                vec![s("false"), s("true")]
            } else {
                values.iter().map(|v| s(v)).collect()
            },
            slider,
            boolean,
        }
    }
    fn model() -> OptionModel {
        OptionModel {
            options: vec![
                opt("SHADOWS", "true", &[], false),
                opt("SUN", "1.0", &["0.5", "1.0", "2.0"], true),
                opt("QUALITY", "1", &["0", "1", "2"], false),
                opt("LOOSE", "a", &["a", "b"], false),
            ],
            screens: [
                (
                    s(""),
                    Screen {
                        entries: vec![
                            Entry::Profile,
                            Entry::Empty,
                            Entry::Screen(s("LIGHT")),
                            Entry::Option(s("QUALITY")),
                            Entry::Rest,
                        ],
                        columns: 0,
                    },
                ),
                (
                    s("LIGHT"),
                    Screen {
                        entries: vec![Entry::Option(s("SHADOWS")), Entry::Option(s("SUN"))],
                        columns: 1,
                    },
                ),
            ]
            .into(),
            profiles: vec![
                Profile {
                    name: s("LOW"),
                    values: [(s("QUALITY"), s("0")), (s("SHADOWS"), s("false"))].into(),
                    ..Default::default()
                },
                Profile {
                    name: s("HIGH"),
                    values: [(s("QUALITY"), s("2")), (s("SHADOWS"), s("true"))].into(),
                    ..Default::default()
                },
            ],
            lang: Lang(
                [
                    (s("option.SUN"), s("Sun")),
                    (s("value.QUALITY.2"), s("§aHigh")),
                    (s("suffix.SUN"), s("x")),
                    (s("screen.LIGHT"), s("Lighting")),
                    (s("profile.LOW"), s("Low")),
                    (s("option.QUALITY.comment"), s("How good. §eSlow.")),
                ]
                .into(),
            ),
            notes: Vec::new(),
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
        assert_eq!(current_profile(&view).as_deref(), Some("HIGH"));
        assert_eq!(next_profile(&view, false).as_deref(), Some("LOW"));
        apply_profile(&mut view, "LOW");
        assert_eq!(profile_label(&view).plain(), "Profile: Low");
        assert_eq!(value_of(&view, "SHADOWS").as_deref(), Some("false"));
    }

    #[test]
    fn a_model_without_the_screen_lists_every_option() {
        let mut m = model();
        m.screens.clear();
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
