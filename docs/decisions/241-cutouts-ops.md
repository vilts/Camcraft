## #241 - the cutouts part: picks, Face, Trace and Contour on an imported mesh

**Reproduction (upstream/main f490458a, the user's `cutouts.camproj`).**
Face, Trace, 2D Contour and 2D Pocket were each created with the GUI's
default parameters and then given one of five pick sets: the planar top
face, all 57 edge chains rimming it, the outer rim's 4 chains, one
hole's 9 chains, or the whole model as the source. What worked already:
the face pick and every edge pick resolved, and Trace, Contour and Pocket
generated on all of them. The Face pick's "15.15 mm gap" refusal is
#260, fixed by PR #313 (merge `1da9695b`) and shipped in v0.15.0; Face
on the face pick generates 726 moves on main. Three failures still
reproduced:

1. **Face on edge picks refused**: "two holes stand 0.63 mm apart ... no
   wider than the 3.000 mm tool". A chain-only Face pick is clipped by the
   part's silhouette (`picks::silhouette`). The slicer refused this
   closed mesh as open, so the silhouette came from its fallback, the
   union of facet shadows. That union was built with `Region::new`, whose
   nesting-parity reading turns a bottom-face facet whose shadow lies
   inside a top-face facet's shadow into a hole. 27 phantom triangular
   "holes", one of them 433 mm², landed in the solid webs, and the facer
   refused on the gaps between them.
2. **The slicer refused a closed mesh** ("open at z = -19.375: 12
   dangling chord ends"). The part's shared corners are not bit-identical;
   copies of one corner differ by up to 1.04 µm. The two facets that share
   an edge therefore compute crossing points a fraction of a micrometre
   apart, and the 1 µm rounded weld key split the pairs that straddled a
   rounding boundary. Every model-sourced silhouette on this part failed,
   and so did Face with the model as its source.
3. **Contour and Trace with the model as source refused** with "no
   material at z = 0.000". Top of Part is the model's top, and the
   highest corner stands 3e-13 mm above the others. A plane at exactly
   that height meets only that one corner, so the section came back empty.

**Decided.**
- The facet-shadow fallback is a pure union (`Region::union_all`, each
  shadow oriented CCW). Every facet shadow is material. `union_all`'s own
  doc already names this case ("the contributions of the facets of ONE
  solid legitimately nest").
- The slicer keeps its exact weld (one rounded 1 µm key per chord end)
  as the first try. Only when that leaves ends dangling ("open") does it
  slice again, welding ends within 2 µm (Chebyshev) through a spatial hash
  over neighbouring cells. Every mesh that sliced before slices exactly
  as before, and only refused meshes are rescued. The code is in the new
  `geometry::slice_weld`, so `slice.rs` (already over 400 lines) does not
  grow by the welder. A real 0.1 mm gap is still refused as open, and that
  is pinned. A first version used the tolerance weld for every slice.
  It turned `blind_floor::a_blind_pocket_never_enters_solid_under_any_ladder`
  red with a Crossing: on that exact mesh, near-tangent chords that the
  exact key kept apart welded together and moved a level's islands. Hence
  the fallback-only design.
- `operation::material::section` already retried 1 µm lower when a slice
  errored. It now does the same when a slice comes back empty. When both
  planes are empty, the refusal names the fix: the Z span the source
  occupies, "set Top of Part Z inside that span, or pick the face or
  edges to machine".

**Why.** Every one of these is a refusal or a wrong region on a valid,
closed part. None of them was a correct refusal. Each fix makes the
answer more correct, never more permissive about real defects. An
open mesh still refuses, and a plane that misses the part still refuses.

**Considered and dropped.**
- Replacing the fallback with a convex hull. That is wrong for parts with
  through-holes.
- Healing the mesh on import (merging the scattered corners). That
  changes the model the user imported, and the slicer is where the
  tolerance belongs.
- A larger weld tolerance (10 µm). It would start fusing real fine
  features. 2 µm is twice the region layer's 1 µm quantization, which is
  already the finest geometry the 2D layer can represent.
- Box selection of edges (the issue's aside). That is a new UI feature,
  not part of this correction, so it is filed as its own enhancement (#979).

**Cost to change later.** Low. The tolerance and bucket size are two
constants in `slice_weld.rs`. The fallback is one function, `facet_shadow`.
The empty-section retry is the existing nudge, and dropping it would
bring back the refusal on noisy tops.
