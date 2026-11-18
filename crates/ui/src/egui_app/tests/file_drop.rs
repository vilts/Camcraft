//! #1450: a model file dropped on the window loads through the Model
//! dialog's own import command (one Ctrl+Z takes it away); a corrupt file,
//! an unsupported one and a multi-file drop are refused by name and leave
//! the project unchanged. The drop is fed as egui's raw dropped-file input.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (#1450): covers ui::egui_app::file_drop, ui::state (refuse_drop, import_path).

use std::path::{Path, PathBuf};

use super::*;

fn cube_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/cube.stl")
}

fn temp_file(tag: &str, name: &str, bytes: &[u8]) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-drop-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    let p = d.join(name);
    std::fs::write(&p, bytes).expect("write the fixture");
    p
}

/// egui's drop handle is a trait the integration owns; the test's own.
#[derive(Debug)]
struct Fake(PathBuf);

impl egui::DroppedFile for Fake {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn drop_files(h: &mut Harness<'static, ShellUi>, paths: &[&Path]) {
    for p in paths {
        h.input_mut().dropped_files.push(std::sync::Arc::new(Fake(p.to_path_buf())));
    }
    h.run_steps(2);
    h.state_mut().state.settle();
    h.run();
}

fn names(h: &Harness<'static, ShellUi>) -> Vec<String> {
    let mut v: Vec<String> = model::named_entities(&h.state().state.world).into_iter().map(|(n, _)| n).collect();
    v.sort();
    v
}

#[test]
fn a_dropped_mesh_imports_through_the_model_command_and_undoes() {
    let mut h = shell_harness();
    let before = names(&h);
    assert!(!before.iter().any(|n| n == "cube"));
    drop_files(&mut h, &[&cube_path()]);
    assert!(names(&h).iter().any(|n| n == "cube"), "the dropped model is in the document: {}", h.state().state.status);
    let st = &mut h.state_mut().state;
    st.history.undo(&st.world);
    assert_eq!(names(&h), before, "one undo removes the dropped model");
}

#[test]
fn a_corrupt_file_is_refused_by_name_and_changes_nothing() {
    let mut h = shell_harness();
    let before = names(&h);
    let bad = temp_file("corrupt", "broken.step", b"this is not a STEP file");
    drop_files(&mut h, &[&bad]);
    let said = h.state().state.status.clone();
    assert!(said.contains("import refused"), "{said}");
    assert_eq!(names(&h), before);
}

#[test]
fn an_unsupported_extension_is_refused_by_name_and_changes_nothing() {
    let mut h = shell_harness();
    let before = names(&h);
    let txt = temp_file("ext", "notes.txt", b"hello");
    drop_files(&mut h, &[&txt]);
    assert!(h.state().state.status.contains("notes.txt is not a model file"), "{}", h.state().state.status);
    assert_eq!(names(&h), before);
}

#[test]
fn several_files_at_once_are_refused_and_change_nothing() {
    let mut h = shell_harness();
    let before = names(&h);
    let other = temp_file("multi", "other.stl", b"x");
    drop_files(&mut h, &[&cube_path(), &other]);
    assert!(h.state().state.status.contains("2 files were dropped"), "{}", h.state().state.status);
    assert_eq!(names(&h), before);
}


mod issue_2064_pins {
    use std::path::PathBuf;
    use camcraft_app::commands::theme_set;
    use camcraft_app::schema::ThemeRef;

    struct TempTheme(PathBuf);

    impl TempTheme {
        fn write(tag: &str, geom: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "camcraft_issue2064_drop_{}_{}_{}.json",
                std::process::id(),
                tag,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ));
            let json = format!(
                r#"{{
  "format": "camcraft-theme/1",
  "descriptor": {{
    "name": "Pin Drop {tag}",
    "creator": "test",
    "description": "issue 2064 pin"
  }},
  "base": "dark",
  "geometry": {{ {geom} }}
}}"#
            );
            std::fs::write(&path, json).expect("write temp theme");
            Self(path)
        }
    }

    impl Drop for TempTheme {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn walk_drop_shapes(
        shape: &egui::Shape,
        borders: &mut Vec<(egui::Rect, egui::CornerRadius, f32)>,
        fonts: &mut Vec<f32>,
    ) {
        match shape {
            egui::Shape::Rect(r) if r.stroke.color == egui::Color32::WHITE && r.stroke.width > 0.0 => {
                borders.push((r.rect, r.corner_radius, r.stroke.width));
            }
            egui::Shape::Text(t) if t.galley.text() == crate::egui_app::file_drop::HINT => {
                if let Some(sec) = t.galley.job.sections.first() {
                    fonts.push(sec.format.font_id.size);
                }
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    walk_drop_shapes(s, borders, fonts);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn drop_hint_reads_window_margin_radius_and_font_from_theme() {
        let _g = super::super::snapshot_guard();
        let mut h = super::super::shell_harness();
        h.run_steps(2);

        let f1 = TempTheme::write("radius_font", r#""radius_window": 14, "font_label": 18.0"#);
        theme_set(
            &h.state_mut().state.world,
            &ThemeRef::File(f1.0.to_string_lossy().into_owned()),
        );
        h.input_mut().hovered_files.push(egui::HoveredFile::default());
        h.run_steps(2);

        let mut borders = Vec::new();
        let mut fonts = Vec::new();
        for c in &h.output().shapes {
            walk_drop_shapes(&c.shape, &mut borders, &mut fonts);
        }
        let (rect1, radius, stroke_w) = borders.pop().expect("drop hint border rect must paint");
        let font_sz = fonts.pop().expect("drop hint label must paint");
        let rx0_f1 = h.state().state.viewport_rect.0[0];
        assert!(
            (rect1.left() - rx0_f1 - 6.0).abs() <= 1.5,
            "default drop hint border inset must equal window_margin=6 (got {})",
            rect1.left() - rx0_f1
        );
        assert_eq!(
            radius,
            egui::CornerRadius::same(14),
            "drop hint border corner radius must read radius_window=14"
        );
        assert!(
            (font_sz - 36.0).abs() <= 0.1,
            "drop hint text size must scale from font_label*2 (expected 36.0, got {font_sz})"
        );
        assert!(
            stroke_w > 2.5,
            "drop hint border stroke must scale with font_label (got {stroke_w})"
        );

        let f2 = TempTheme::write("margin14", r#""window_margin": 14.0"#);
        theme_set(
            &h.state_mut().state.world,
            &ThemeRef::File(f2.0.to_string_lossy().into_owned()),
        );
        h.run_steps(2);
        let rx0 = h.state().state.viewport_rect.0[0];
        let mut borders2 = Vec::new();
        let mut fonts2 = Vec::new();
        for c in &h.output().shapes {
            walk_drop_shapes(&c.shape, &mut borders2, &mut fonts2);
        }
        let (rect2, _, _) = borders2.pop().expect("drop hint border rect must paint");
        assert!(
            (rect2.left() - rx0 - 14.0).abs() <= 1.5,
            "drop hint border inset must read window_margin=14 (got {})",
            rect2.left() - rx0
        );

        h.input_mut().hovered_files.clear();
        theme_set(&h.state_mut().state.world, &ThemeRef::Dark);
        h.run_steps(2);
    }
}

#[derive(Debug)]
struct BareDrop {
    name: PathBuf,
    data: Result<Vec<u8>, String>,
}

impl egui::DroppedFile for BareDrop {
    fn path(&self) -> &Path {
        &self.name
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        self.data.clone()
    }
}

fn drop_bare(h: &mut Harness<'static, ShellUi>, name: &str, data: Result<Vec<u8>, String>) {
    h.input_mut().dropped_files.push(std::sync::Arc::new(BareDrop {
        name: PathBuf::from(name),
        data,
    }));
    h.run_steps(2);
    h.state_mut().state.settle();
    h.run();
}

#[test]
fn a_bare_name_dropped_mesh_with_bytes_imports_and_undoes() {
    let mut h = shell_harness();
    let before = names(&h);
    assert!(!before.iter().any(|n| n == "cube"));
    let bytes = std::fs::read(cube_path()).expect("cube.stl");
    drop_bare(&mut h, "cube.stl", Ok(bytes));
    assert!(
        names(&h).iter().any(|n| n == "cube"),
        "the bare-name dropped model is in the document: {}",
        h.state().state.status
    );
    let st = &mut h.state_mut().state;
    st.history.undo(&st.world);
    assert_eq!(names(&h), before, "one undo removes the dropped model");
}

#[test]
fn a_bare_name_drop_with_bad_extension_or_read_error_is_refused_by_name() {
    let mut h = shell_harness();
    let before = names(&h);
    drop_bare(&mut h, "notes.txt", Ok(b"hello".to_vec()));
    assert!(
        h.state().state.status.contains("notes.txt is not a model file"),
        "{}",
        h.state().state.status
    );
    assert_eq!(names(&h), before);

    drop_bare(&mut h, "unreadable.stl", Err("browser read error".into()));
    let said = h.state().state.status.clone();
    assert!(
        said.contains("unreadable.stl") && said.contains("browser read error"),
        "{said}"
    );
    assert_eq!(names(&h), before);

    drop_bare(&mut h, "broken.step", Ok(b"this is not a STEP file".to_vec()));
    let said = h.state().state.status.clone();
    assert!(said.contains("import refused"), "{said}");
    assert_eq!(names(&h), before);
}
