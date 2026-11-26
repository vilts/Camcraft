//! #2958: the wide overlay lines' CPU half - the instance bytes, the
//! pixel width and the sample-count fallback. The GPU half (full width on
//! a face, closed corners, MSAA) is in `tests/gpu_render.rs`.
//! Provenance: our own tests, specced from the issue's "Done means".
//!
//! Gate map (M4.4): covers ui::viewport (wide).

use super::*;

#[test]
fn each_segment_is_one_32_byte_instance() {
    let v = |x: f32, rgb: [f32; 3]| [x, 2.0, 3.0, rgb[0], rgb[1], rgb[2]];
    let lines = [v(1.0, [1.0, 0.0, 0.5]), v(4.0, [0.0, 1.0, 0.0]), v(7.0, [0.2, 0.2, 0.2]), v(8.0, [0.2, 0.2, 0.2])];
    let bytes = segment_bytes(&lines);
    assert_eq!(bytes.len() as u64, 2 * SEGMENT_BYTES, "two segments, 32 bytes each");
    let f = |o: usize| f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    assert_eq!([f(0), f(4), f(8), f(12), f(16), f(20)], [1.0, 2.0, 3.0, 4.0, 2.0, 3.0], "both points as f32");
    assert_eq!(bytes[24..28], [255, 0, 128, 255], "the start colour as RGBA8");
    assert_eq!(bytes[28..32], [0, 255, 0, 255], "the end keeps its own colour (the grid fades along a line)");
    // The 512k-segment overlay ceiling fits in 16 MiB (the old 1 px
    // vertex pairs used 24 MiB).
    assert_eq!(crate::interact::MAX_OVERLAY_VERTICES as u64 / 2 * SEGMENT_BYTES, 16 * 1024 * 1024);
}

#[test]
fn an_odd_vertex_and_bad_colours_never_reach_the_gpu_as_garbage() {
    let odd = [[0.0; 6], [1.0; 6], [2.0; 6]];
    assert_eq!(segment_bytes(&odd).len() as u64, SEGMENT_BYTES, "the trailing half line is dropped");
    assert!(segment_bytes(&[]).is_empty());
    let bad = [[0.0, 0.0, 0.0, f32::NAN, 2.0, -1.0], [0.0, 0.0, 0.0, f32::INFINITY, 0.0, 0.0]];
    let b = segment_bytes(&bad);
    assert_eq!(b[24..28], [0, 255, 0, 255], "NaN reads 0, out of range clamps");
    assert_eq!(b[28..32], [255, 0, 0, 255]);
}

#[test]
fn the_line_width_scales_with_the_display_and_stays_in_range() {
    assert!((line_px(1.5, 1.0) - 1.5).abs() < 1e-6);
    assert!((line_px(1.5, 1.5) - 2.25).abs() < 1e-6, "a 150 % display widens the line");
    assert!((line_px(1.0, 0.5) - MIN_LINE_PX).abs() < 1e-6, "never under one pixel");
    assert!((line_px(3.0, 100.0) - MAX_LINE_PX).abs() < 1e-6, "never past the cap");
    assert!((line_px(1.5, f64::NAN) - MIN_LINE_PX).abs() < 1e-6, "a bad scale draws one pixel");
    assert!((line_px(f32::INFINITY, 1.0) - MIN_LINE_PX).abs() < 1e-6);
}

#[test]
fn an_unoffered_sample_count_falls_back_to_the_highest_offered_below_it() {
    let guaranteed = GUARANTEED_SAMPLES.to_vec();
    assert_eq!(effective_samples(4, &guaranteed), 4);
    assert_eq!(effective_samples(1, &guaranteed), 1);
    assert_eq!(effective_samples(2, &guaranteed), 1, "2x is not guaranteed: never more memory than asked");
    assert_eq!(effective_samples(2, &[1, 2, 4]), 2, "a device that offers 2x gets it");
    assert_eq!(effective_samples(4, &[]), 1, "nothing offered: one sample");
    assert_eq!(effective_samples(0, &guaranteed), 1);
}

#[test]
fn the_shader_lifts_by_the_documented_widths() {
    let lift = format!("u.vp.w * {LIFT_WIDTHS:.1}");
    assert!(WIDE_SHADER.contains(&lift), "the shader's lift is LIFT_WIDTHS ({lift})");
}
