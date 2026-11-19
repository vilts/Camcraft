//! #1122: form-seam guard tests — migrated holes pages stay at zero raw
//! egui control calls. Unmigrated families are outside this PR’s guard.
//! Provenance: our own source-level guard over `crates/ui/src/egui_app`.

const RAW_PATTERNS: &[&str] = &[
    "ui.button(",
    "ui.small_button(",
    "ui.checkbox(",
    "ui.label(",
    "ui.add(",
    "ui.add_enabled(",
    "ui.add_sized(",
    "ui.radio_value(",
];

fn count_raw_calls(src: &str) -> usize {
    src.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .map(|line| {
            RAW_PATTERNS
                .iter()
                .map(|pat| line.matches(pat).count())
                .sum::<usize>()
        })
        .sum()
}

struct Family<'a> {
    name: &'a str,
    max_raw: usize,
    sources: &'a [(&'a str, &'a str)],
}

const FAMILIES: &[Family<'_>] = &[
    Family {
        name: "holes",
        max_raw: 0,
        sources: &[
            ("holes_page.rs", include_str!("../holes_page.rs")),
            ("thread_page.rs", include_str!("../thread_page.rs")),
        ],
    },

];

#[test]
fn form_seam_guard_covers_only_the_migrated_holes_family() {
    assert_eq!(FAMILIES.len(), 1, "unmigrated families must not block the holes migration");
    assert_eq!(FAMILIES[0].name, "holes");
    assert_eq!(FAMILIES[0].max_raw, 0);
    let files: Vec<_> = FAMILIES[0].sources.iter().map(|(name, _)| *name).collect();
    assert_eq!(files, ["holes_page.rs", "thread_page.rs"]);
}

#[test]
fn pages_do_not_add_raw_egui_calls_outside_the_form_seam() {
    let mut violations = Vec::new();
    for fam in FAMILIES {
        let mut total = 0;
        let mut per_file = Vec::new();
        for (name, src) in fam.sources {
            let n = count_raw_calls(src);
            if n > 0 {
                per_file.push(format!("{name}={n}"));
            }
            total += n;
        }
        if total > fam.max_raw {
            violations.push(format!(
                "family '{}' has {total} raw ui calls (limit {}): {}",
                fam.name,
                fam.max_raw,
                per_file.join(", ")
            ));
        }
    }
    assert!(violations.is_empty(), "raw egui calls bypass the form seam:\n{}", violations.join("\n"));
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Choice {
    First,
    Second,
}

struct SeamProbe {
    hits: usize,
    small_hits: usize,
    choice: Choice,
    count: u32,
}

impl Default for SeamProbe {
    fn default() -> Self {
        Self { hits: 0, small_hits: 0, choice: Choice::Second, count: 1 }
    }
}

#[test]
fn form_seam_controls_preserve_labels_clicks_and_accessible_names() {
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    let mut h = Harness::builder().with_size(egui::Vec2::new(480.0, 260.0)).build_ui_state(
        |ui, p: &mut SeamProbe| {
            super::list_row(ui, "1  X 10.000  Y 20.000");
            if super::small_button(ui, "Remove 1").clicked() {
                p.small_hits += 1;
            }
            if super::button(ui, "Detect holes").clicked() {
                p.hits += 1;
            }
            super::prop_row(ui, "Hand", |ui| {
                super::radio_pair(ui, &mut p.choice, (Choice::First, "Left hand"), (Choice::Second, "Right hand"));
            });
            super::prop_row(ui, "Spring passes", |ui| {
                super::super::fields::int_range_row(ui, &mut p.count, 0..=4);
            });
        },
        SeamProbe::default(),
    );
    h.run();

    assert!(h.query_by_label("1  X 10.000  Y 20.000").is_some());
    assert!(h.query_by_label("Left hand").is_some() && h.query_by_label("Right hand").is_some());
    let spin_named = h.root().children_recursive().any(|n| {
        let node = n.accesskit_node();
        node.role() == Role::SpinButton && node.labelled_by().next().is_some()
    });
    assert!(spin_named, "int_range_row inside prop_row must carry labelled_by");

    h.get_by_label("Detect holes").click();
    h.get_by_label("Remove 1").click();
    h.get_by_label("Left hand").click();
    h.run();

    assert_eq!(h.state().hits, 1);
    assert_eq!(h.state().small_hits, 1);
    assert_eq!(h.state().choice, Choice::First);
}

// #2295: a wide property pane must use its free space for field names.
fn assert_property_label_uses_spare_width(checkbox: bool) {
    use egui_kittest::Harness;

    const LABEL: &str = "Maximum spindle acceleration";
    let mut h = Harness::builder().with_size(egui::Vec2::new(560.0, 100.0)).build_ui_state(
        move |ui, value: &mut bool| {
            if checkbox {
                super::check_row(ui, LABEL, value);
            } else {
                super::prop_row(ui, LABEL, |ui| {
                    ui.add_sized(egui::Vec2::new(super::CTRL_W, 20.0), egui::Button::new("Value"));
                });
            }
        },
        false,
    );
    h.run();

    fn label_galleys(shape: &egui::Shape, out: &mut Vec<bool>) {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == LABEL => out.push(text.galley.elided),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| label_galleys(shape, out)),
            _ => {}
        }
    }
    let mut elided = Vec::new();
    for clipped in &h.output().shapes {
        label_galleys(&clipped.shape, &mut elided);
    }
    assert_eq!(elided.len(), 1, "the row must paint the property label");
    assert!(!elided[0], "the property label is cut off while the 560-point pane has free space");
}

#[test]
fn property_label_uses_spare_panel_width() {
    assert_property_label_uses_spare_width(false);
}

#[test]
fn checkbox_label_uses_spare_panel_width() {
    assert_property_label_uses_spare_width(true);
}

#[test]
fn property_controls_fit_a_narrow_panel_and_checkbox_caption_toggles() {
    use egui_kittest::kittest::Queryable;
    for width in [200.0, 260.0] {
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::Vec2::new(width, 100.0))
            .build_ui_state(
                |ui, checked: &mut bool| {
                    let pane = ui.max_rect();
                    let value = super::prop_row(ui, "Maximum spindle acceleration", |ui| {
                        ui.add_sized(egui::Vec2::new(super::CTRL_W, 20.0), egui::Button::new("Value"))
                    });
                    assert!(value.rect.width() >= super::CTRL_W, "the label must reserve the compact control width");
                    let checkbox = super::check_row(ui, "Synchronize spindle acceleration", checked);
                    for rect in [value.rect, checkbox.rect] {
                        assert!(pane.contains_rect(rect), "control {rect:?} extends outside pane {pane:?}");
                    }
                },
                false,
            );
        h.run();
        let label = h.get_by_role_and_label(egui::accesskit::Role::Label, "Maximum spindle acceleration").rect();
        let value = h.get_by_role_and_label(egui::accesskit::Role::Button, "Value").rect();
        assert!(label.right() <= value.left(), "label {label:?} overlaps control {value:?} at panel width {width}");
        h.get_by_role_and_label(egui::accesskit::Role::Label, "Synchronize spindle acceleration").click();
        h.run();
        assert!(*h.state(), "clicking the caption must still toggle the checkbox");
    }
}

#[test]
fn truncated_property_label_shows_full_name_on_hover() {
    use egui_kittest::kittest::Queryable;

    const LABEL: &str = "Maximum spindle acceleration";
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::Vec2::new(260.0, 100.0))
        .build_ui(|ui| {
            super::prop_row(ui, LABEL, |ui| {
                ui.add_sized(egui::Vec2::new(super::CTRL_W, 20.0), egui::Button::new("Value"));
            });
        });
    h.run();
    let center = h.get_by_role_and_label(egui::accesskit::Role::Label, LABEL).rect().center();
    h.ctx.style_mut_of(h.ctx.theme(), |style| style.interaction.tooltip_delay = 0.0);
    h.hover_at(center);
    h.run_steps(3);

    fn has_full_label(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Text(text) => text.galley.text() == LABEL && !text.galley.elided,
            egui::Shape::Vec(shapes) => shapes.iter().any(has_full_label),
            _ => false,
        }
    }
    assert!(
        h.output().shapes.iter().any(|clipped| has_full_label(&clipped.shape)),
        "hover must show the full name when the label must be cut off"
    );
}
