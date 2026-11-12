//! The UI icon set, folded into the egui stack at M1.4.2 M-g: bundled
//! 32 px PNGs, recolored to white line art and loaded as egui textures.
//!
//! Provenance: the loader is our own, ported from the retired imgui
//! icons.rs. The chrome glyphs are Tabler Icons (MIT, see
//! assets/ui/LICENSES.md), unmodified except channel order; the
//! operation glyphs (#661) are our own drawings, SVG masters under
//! assets/ui/svg/ rasterised by `cargo xtask icons`, as are the tool-kind,
//! tree-row, status-badge and action glyphs since #661 slice 2 (AGENTS.md clean-room:
//! never any HSMWorks or other CAM product's asset).
//!
//! The imgui loader hand-built wgpu textures and registered them as
//! external ids; egui owns that machinery, so `TextureHandle`s come from
//! `ctx.load_texture` (the renderer's texture manager uploads and
//! re-uploads as needed). The law carried over unchanged: building the
//! set is a lazy GPU job (a headless or pre-GPU frame holds `None`), a
//! glyph that fails to decode is dropped to `None` with a diagnostic -
//! never the whole ribbon - and every consumer must degrade to its
//! painted fallback, never to nothing.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

/// One table row per slot: the variant, its PNG stem under assets/ui/
/// (an `op-*` stem has an SVG master under assets/ui/svg/), and its egui
/// texture key - stable per slot, never reused for other pixels (a rename
/// would alias old uploads). The table lives once; the enum, `ALL`, the
/// bytes and the keys are all generated from it.
macro_rules! icon_table {
    ($( $(#[$m:meta])* $v:ident => $file:literal, $key:literal; )*) => {
        /// One bundled glyph. The ribbon is the first consumer; the
        /// browser tree pass added slots here, not elsewhere.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub(crate) enum Icon { $( $(#[$m])* $v, )* }

        impl Icon {
            /// Every slot, ribbon order - the table and the store share this list.
            pub(crate) const ALL: &'static [Icon] = &[ $( Icon::$v, )* ];

            fn png(self) -> &'static [u8] {
                match self {
                    $( Icon::$v => include_bytes!(concat!("../../../../assets/ui/", $file, ".png")), )*
                }
            }

            /// The PNG stem under assets/ui/ (pinned by the structural tests:
            /// every operation glyph is an `op-*` file with an SVG master).
            #[cfg(test)]
            pub(crate) fn file(self) -> &'static str {
                match self { $( Icon::$v => $file, )* }
            }

            fn name(self) -> &'static str {
                match self { $( Icon::$v => $key, )* }
            }
        }
    };
}

icon_table! {
    New => "file-plus", "icon-new";
    OpenModel => "folder-open", "icon-open-model";
    OpenProject => "file-import", "icon-open-project";
    Save => "device-floppy", "icon-save";
    // The CamCraft actions and tree rows (#661 slice 2): original SVG
    // masters too - `act-*` for the actions, `tree-*` for the browser's
    // setup rows.
    /// A program page with the arrow out of it.
    Post => "act-post", "icon-post";
    /// The isometric block with its hidden edges dashed: raw material.
    Stock => "tree-stock", "icon-stock";
    /// The origin dot and its X/Y arrows.
    WorkOffset => "tree-wcs", "icon-work-offset";
    Tools => "tools", "icon-tools";
    // The operation glyphs (#661): one per `OpType`, every one an
    // original SVG master in assets/ui/svg/ - a cutter and its path,
    // drawn from what the operation does. 2D milling:
    Face => "op-face", "icon-face";
    Contour => "op-contour", "icon-contour";
    Pocket => "op-pocket", "icon-pocket";
    Adaptive2D => "op-adaptive2d", "icon-adaptive2d";
    Offset2D => "op-offset2d", "icon-offset2d";
    Slot => "op-slot", "icon-slot";
    TrochoidalSlot => "op-trochoidal-slot", "icon-trochoidal-slot";
    Trace => "op-trace", "icon-trace";
    Engrave => "op-engrave", "icon-engrave";
    Chamfer2D => "op-chamfer2d", "icon-chamfer2d";
    Deburr => "op-deburr", "icon-deburr";
    Circular => "op-circular", "icon-circular";
    PlasmaProfile => "op-plasma-profile", "icon-plasma-profile";
    // Holes and probing:
    Drill => "op-drill", "icon-drill";
    Bore => "op-bore", "icon-bore";
    Thread => "op-thread", "icon-thread";
    /// The touch probe (#108): a stylus over its ball.
    Probe => "op-probe", "icon-probe";
    // 3D milling:
    Pocket3D => "op-pocket3d", "icon-pocket3d";
    Finish3D => "op-finish3d", "icon-finish3d";
    // Multi-axis: the dome with the ball leaning on its normal; the
    // swarf blade with the tool lying on it; the rotary bar with the
    // tool riding outside it on the radial.
    Contour5D => "op-contour5d", "icon-contour5d";
    Parallel5D => "op-parallel5d", "icon-parallel5d";
    Swarf => "op-swarf", "icon-swarf";
    AdvancedSwarf => "op-advanced-swarf", "icon-advanced-swarf";
    RotaryContour => "op-rotary-contour", "icon-rotary-contour";
    RotaryParallel => "op-rotary-parallel", "icon-rotary-parallel";
    RotaryOffset => "op-rotary-offset", "icon-rotary-offset";
    MultiAxisRough => "op-multiaxis-rough", "icon-multiaxis-rough";
    Flow5D => "op-flow5d", "icon-flow5d";
    MultiAxisContour => "op-multiaxis-contour", "icon-multiaxis-contour";
    // Turning: the chuck block at the left, the bar, an insert or blade.
    Facing => "op-facing", "icon-facing";
    ProfileRough => "op-profile-rough", "icon-profile-rough";
    ProfileFinish => "op-profile-finish", "icon-profile-finish";
    GrooveRough => "op-groove-rough", "icon-groove-rough";
    GrooveFinish => "op-groove-finish", "icon-groove-finish";
    PartOff => "op-part-off", "icon-part-off";
    Threading => "op-threading", "icon-threading";
    Chamfering => "op-chamfering", "icon-chamfering";
    TaperTurning => "op-taper-turning", "icon-taper-turning";
    MultiPassTurning => "op-multipass-turning", "icon-multipass-turning";
    FormTurning => "op-form-turning", "icon-form-turning";
    BoringTurning => "op-boring-turning", "icon-boring-turning";
    Undo => "arrow-back-up", "icon-undo";
    Redo => "arrow-forward-up", "icon-redo";
    Delete => "trash", "icon-delete";
    /// A raster path with its arrowhead and a spark: make the toolpath.
    Generate => "act-generate", "icon-generate";
    FrameAll => "act-frame-all", "icon-frame-all";
    Toolpaths => "act-show-paths", "icon-toolpaths";
    /// The machined part: the block with a bore in its face.
    Model => "tree-part", "icon-model";
    EyeOff => "act-hide-paths", "icon-eye-off";
    /// The play mark with speed lines: run the move simulation.
    Simulate => "act-simulate", "icon-simulate";
    /// The vise: two jaws over the bed, the part between, the screw bar.
    Fixture => "tree-fixture", "icon-fixture";
    /// The status strip's Debug info door (#157).
    DebugInfo => "act-debug-info", "icon-debug-info";
    /// A setup: the setup sheet with its origin corner.
    Setup => "tree-setup", "icon-setup";
    /// The machine: column, head and spindle over the table.
    Machine => "tree-machine", "icon-machine";
    /// A setup's Operations node: the folder with a toolpath in it.
    OpGroup => "tree-op-group", "icon-op-group";
    // Operation status badges (#661 section 7): solid shapes with a
    // knocked-out mark, painted small over an op row's glyph.
    BadgeNever => "badge-never", "icon-badge-never";
    BadgeOk => "badge-ok", "icon-badge-ok";
    BadgeStale => "badge-stale", "icon-badge-stale";
    BadgeWarning => "badge-warning", "icon-badge-warning";
    // Tool kinds (#661 section 4), one per `ToolKind`, drawn from the
    // cutter's silhouette: mill tools hang from a solid holder block,
    // lathe tools run left from a solid tool-post block at the right.
    ToolEndMill => "tool-end-mill", "icon-tool-end-mill";
    ToolBallNose => "tool-ball-nose", "icon-tool-ball-nose";
    ToolBullNose => "tool-bull-nose", "icon-tool-bull-nose";
    ToolDrill => "tool-drill", "icon-tool-drill";
    ToolTap => "tool-tap", "icon-tool-tap";
    ToolBoringBar => "tool-boring-bar", "icon-tool-boring-bar";
    ToolTurningInsert => "tool-turning-insert", "icon-tool-turning-insert";
    ToolGroovingBlade => "tool-grooving-blade", "icon-tool-grooving-blade";
    ToolThreadingInsert => "tool-threading-insert", "icon-tool-threading-insert";
    ToolForm => "tool-form", "icon-tool-form";
    ToolChamferMill => "tool-chamfer-mill", "icon-tool-chamfer-mill";
    ToolProbe => "tool-probe", "icon-tool-probe";
    ToolTorch => "tool-torch", "icon-tool-torch";
}

/// Upload edge: 2x the token's `glyph_px` (20 display px at the dark
/// defaults - the painted path reads the token, not this number) for
/// clean minification on the button glyphs.
const UPLOAD_PX: u32 = 40;

/// Loaded icon set: one handle per slot, parallel to [`Icon::ALL`], with
/// `None` where a glyph failed to decode. A headless frame never builds
/// one at all, so every consumer takes `Option<&IconSet>`.
#[derive(Clone)]
pub(crate) struct IconSet {
    handles: Vec<Option<TextureHandle>>,
}

/// Where `load` leaves a copy of the set (the handles are shared, not the
/// pixels) for panes that are not handed one: the property pages draw the
/// tool kind glyphs through it (#661).
fn ctx_key() -> egui::Id {
    egui::Id::new("camcraft-icon-set")
}

impl IconSet {
    /// Decode, recolor and register every bundled glyph. Called once,
    /// lazily, from the first GPU-backed frame.
    pub(crate) fn load(ctx: &Context) -> Self {
        let handles = Icon::ALL
            .iter()
            .map(|icon| match Self::upload(ctx, *icon) {
                Ok(handle) => Some(handle),
                Err(err) => {
                    diag_warn!("icon {icon:?} unavailable: {err}");
                    None
                }
            })
            .collect();
        let set = IconSet { handles };
        ctx.data_mut(|d| d.insert_temp(ctx_key(), std::sync::Arc::new(set.clone())));
        set
    }

    /// The set `load` built for this context (`None` before the first
    /// GPU-backed frame, and headless): every caller degrades to its text.
    pub(crate) fn from_ctx(ctx: &Context) -> Option<std::sync::Arc<IconSet>> {
        ctx.data(|d| d.get_temp(ctx_key()))
    }

    fn upload(ctx: &Context, icon: Icon) -> Result<TextureHandle, String> {
        let img = image::load_from_memory(icon.png())
            .map_err(|e| e.to_string())?
            .into_rgba8();
        let small = image::imageops::resize(
            &img,
            UPLOAD_PX,
            UPLOAD_PX,
            image::imageops::FilterType::Lanczos3,
        );
        // Tabler strokes are black with alpha coverage; the chrome is dark,
        // so repaint every pixel white and let the alpha do the drawing.
        let mut rgba = small.to_vec();
        for px in rgba.as_chunks_mut::<4>().0 {
            px[0] = 255;
            px[1] = 255;
            px[2] = 255;
        }
        let color_image = ColorImage::from_rgba_unmultiplied([UPLOAD_PX as usize; 2], &rgba);
        Ok(ctx.load_texture(
            icon.name(),
            color_image,
            TextureOptions::default(),
        ))
    }

    /// Texture for a slot (`None` after a failed decode; every consumer
    /// must degrade to its painted fallback, never to nothing).
    pub(crate) fn texture(&self, icon: Icon) -> Option<&TextureHandle> {
        let i = Icon::ALL.iter().position(|s| *s == icon)?;
        self.handles.get(i)?.as_ref()
    }
}
