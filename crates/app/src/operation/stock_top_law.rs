//! Issue #1091: generate's wiring of `verify::acquit_by_stock_top` - a vertical dive the column law convicted against the stock's highest point is re-read against the stock and part solids under the tool; our own reading of the shop rule "never rapid into uncut stock", no vendor material consulted.

use crate::cl::Toolpath;
use crate::geometry::Triangle;
use crate::tools::ToolGeom;
use crate::verify::{self, Mode, Warning};

use super::material::Z_TOL;

/// Re-read `warns`' dive convictions against the solids that bound what
/// the op can meet, in the path's own frame: `stock`, the stock the
/// column law judged (`material_source::judged_stock`), and `part`, the
/// op's own material. With no judged stock the column law read the op's
/// region in its place - no solid bounds that reading, so every
/// conviction stands. The caller passes no stock either when the stock's
/// chord is unknown (#1091 review, #1056: a from-model copy whose source
/// is gone - `model_fidelity::stock_fidelity_known`). `excess` is the op's model excess (#864/#904):
/// the material may stand that much outside its facets. Only removes
/// convictions (see the sim's module).
pub(super) fn acquit(
    path: &Toolpath,
    mode: Mode,
    stock: Option<&[Triangle]>,
    part: &[Triangle],
    tool: &ToolGeom,
    excess: f64,
    warns: &mut Vec<Warning>,
) {
    let Some(stock) = stock else { return };
    if !warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })) {
        return;
    }
    verify::acquit_by_stock_top(path, mode, &[stock, part], tool, Z_TOL, excess, warns);
}
