//! Tool preview paint laws for a body replacing the parameter holder (#2005).
//! Provenance: our own egui shapes from the editor preview; no pixel snapshots.

use super::*;

fn painted(body: Option<&[(f64, f64, f64)]>) -> Vec<Shape> {
    let mut tool = crate::state::ToolUi::default().to_tool();
    tool.diameter = 8.0;
    tool.flute_len = 20.0;
    tool.shank_d = 8.0;
    tool.shank_len = 40.0;
    tool.holder_d = 60.0;
    tool.holder_len = 60.0;
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        draw_tool_preview(ui, &tool, Unit::Millimeters, body);
    });
    output.textures_delta.clear();
    fn flatten(shape: Shape, out: &mut Vec<Shape>) {
        match shape {
            Shape::Vec(shapes) => shapes.into_iter().for_each(|s| flatten(s, out)),
            shape => out.push(shape),
        }
    }
    let mut shapes = Vec::new();
    for clipped in output.shapes {
        flatten(clipped.shape, &mut shapes);
    }
    shapes
}

fn holder_fills(shapes: &[Shape]) -> usize {
    shapes
        .iter()
        .filter(|s| matches!(s, Shape::Path(p) if p.fill == Color32::from_rgb(86, 92, 104)))
        .count()
}

#[test]
fn tool_preview_without_body_keeps_parameter_holder() {
    let shapes = painted(None);
    assert_eq!(
        holder_fills(&shapes),
        1,
        "the parameter holder is the displayed holder without a body"
    );
    assert!(
        !shapes.iter().any(
            |s| matches!(s, Shape::Rect(r) if r.stroke.color == Color32::from_rgb(230, 160, 60))
        )
    );
}

#[test]
fn tool_preview_body_replaces_parameter_holder_fill() {
    let shapes = painted(Some(&[(60.0, 20.0, 12.0)]));
    assert_eq!(
        holder_fills(&shapes),
        0,
        "a body replaces the parameter holder instead of overlapping it"
    );
    assert!(
        shapes.iter().any(
            |s| matches!(s, Shape::Rect(r) if r.stroke.color == Color32::from_rgb(230, 160, 60))
        )
    );
}

#[test]
fn tool_preview_body_replaces_parameter_holder_outline() {
    let shapes = painted(Some(&[(60.0, 20.0, 12.0)]));
    let body = shapes
        .iter()
        .find_map(|s| match s {
            Shape::Rect(r) if r.stroke.color == Color32::from_rgb(230, 160, 60) => Some(r.rect),
            _ => None,
        })
        .expect("the body is drawn");
    let outline = shapes
        .iter()
        .find_map(|s| match s {
            Shape::Path(p)
                if p.stroke.color
                    == egui::epaint::ColorMode::Solid(Color32::from_rgb(214, 220, 228)) =>
            {
                Some(&p.points)
            }
            _ => None,
        })
        .expect("the cutter and shank outline is drawn");
    assert!(
        outline.iter().all(|p| p.y >= body.bottom() - 0.01),
        "the parameter holder outline must stop at the shank top"
    );
    assert!(
        outline
            .iter()
            .all(|p| p.x > body.left() && p.x < body.right()),
        "the removed parameter holder must not widen the cutter outline"
    );
}

#[test]
fn tool_preview_body_sets_frame_without_parameter_holder() {
    let shapes = painted(Some(&[(60.0, 20.0, 12.0)]));
    let rect = |color| {
        shapes
            .iter()
            .find_map(|s| match s {
                Shape::Rect(r) if r.fill == color => Some(r.rect),
                _ => None,
            })
            .expect("the preview rectangle is painted")
    };
    let background = rect(Color32::from_rgb(24, 26, 31));
    let body = rect(Color32::from_rgba_unmultiplied(230, 160, 60, 70));
    assert!(
        (body.top() - background.top() - 18.0).abs() < 0.01,
        "the shorter body determines the height, with 18 px padding: background {background:?}, body {body:?}"
    );
}
