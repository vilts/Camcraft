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

