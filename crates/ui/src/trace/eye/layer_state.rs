//! What the layer playback asks of the stock eye (#3336, GPU stock sim
//! S3c): whether it traces on the GPU, and whether the frame that it
//! holds is resident there. Split from `eye.rs` (the 400-line law).
//! Provenance: our own code over the eye's own state.

use std::sync::Arc;

use camcraft_app::stock_view::FieldView;

use super::StockEye;

impl StockEye {
    /// The eye has a GPU trace pass: a packed playback frame is drawn
    /// from its packed layers, not from `FieldView::stock`.
    #[must_use]
    pub const fn traces_on_gpu(&self) -> bool {
        self.trace.is_some()
    }

    /// `Some(on_gpu)` when the eye holds `fv` as its resident view:
    /// `false` when the CPU marches it (it then reads `FieldView::stock`).
    /// `None` when the eye holds another view or none.
    #[must_use]
    pub fn holds_on_gpu(&self, fv: &Arc<FieldView>) -> Option<bool> {
        self.source.as_ref().filter(|s| Arc::ptr_eq(s, fv)).map(|_| self.gpu_scene)
    }
}
