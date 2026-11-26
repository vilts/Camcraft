//! CamCraft app layer - a clean-room implementation.
//!
//! Provenance: independent implementation specced solely from public
//! documentation of commercial CAM workflows and published technical
//! literature. No HSMWorks source, binaries, or assets were consulted.

pub use camcraft_cl::cl;
pub mod cli;
pub mod cli_skill;
pub mod cli_ui;
pub mod clock;
pub mod commands;
// #3073: saved cut recipes - an operation's settings under a name.
pub mod cut_recipe;
pub mod cutting;
// #890: which row of a tool's per-material cutting data the document picks.
pub mod cutting_rows;
// #891: turning cutting data - CSS ops, working diameter, the suggestion.
pub mod cutting_turn;
pub mod smoothing;
pub use camcraft_motion::drilling;
pub mod fixture;
// #744: editable spun & extruded fixture profiles + DXF/STEP integration.
pub mod fixture_profile;
// #339 slice 6: the user fixture library file.
pub mod fixture_library;
pub mod step_source;
pub mod model_source;
pub mod fixture_carried;
// #561: locate the part in a fixture (or the fixture on the part) by mates.
pub mod fixture_clash;
pub mod fixture_locate;
pub mod fixture_struck;
pub mod fixture_view;
pub mod vise;
pub mod vise_fit;
pub mod accessory;
pub mod plate;
pub mod modvise;
pub use camcraft_motion::finishing3d;
pub use camcraft_geometry::geo2d;
pub mod geometry;
pub use camcraft_motion::links;
pub use camcraft_motion::path_picks;
pub use camcraft_motion::load;
pub use camcraft_motion::machining2d;
pub use camcraft_motion::machining3d;
pub use camcraft_motion::multiaxis;
pub use camcraft_motion::turning;
pub mod logging;
pub mod machine;
// #25: deviation classes on the machined mesh.
pub mod machined_classes;
pub mod mcp;
// #1945: AI provider endpoints, wire formats, keys and the HTTP client.
pub mod ai;
pub mod model;
// #864: models imported coarser than the importer promises.
pub mod model_fidelity;
// #2996: what a model checked at reduced accuracy says.
pub mod fidelity_note;
pub mod mesh_open;
pub mod mesh_solids;
pub mod model_vouch;
pub mod operation;
pub mod pick;
pub use camcraft_post::post;
pub mod path_sig;
pub mod posting;
// #2950: the browser's own .mch and .cps files, kept for the session.
pub use camcraft_post::session_files;
// #530: the op-family door at post.
mod posting_family;
/// Which spindle a lathe post turns for a section, and the door that
/// keeps a milling operation off the work spindle (#1190).
pub mod lathe_voice;
// #155: standalone Pause / Home entries in the operation list.
pub mod program_entry;
pub use camcraft_motion::plasma;
// #1883: the waterjet profile and the 2D sheet kernel it shares.
pub use camcraft_motion::{sheet, waterjet};
pub use camcraft_motion::wire_edm;
pub use camcraft_motion::probing;
// #541: sinker EDM burn to depth.
pub use camcraft_motion::sinker;
// #543: sinker rough and finish with undersize.
pub use camcraft_motion::sinker_undersize;
// #547: electrode generation from part faces.
pub use camcraft_motion::sinker_cavity;
pub use camcraft_motion::sinker_electrode;
pub use camcraft_motion::sinker_wear;
// #544: sinker EDM orbit patterns.
pub use camcraft_motion::sinker_orbit;
// #545: sinker EDM side burning.
pub use camcraft_motion::sinker_side;
// #3333: the start and exit picks of the sinker kinds.
pub use camcraft_motion::sinker_picks;
pub mod project;
// #35: pocket and slot recognition on the model's faces.
pub mod recognize;
// #182: the CPU rasteriser behind the MCP stock render.
pub mod raster;
// #877: the theme and colour-vision mode, kept per user (not per project).
pub mod prefs;
mod prefs_setups;
// #2428: startup update check against GitHub Releases (native only for HTTP).
pub mod update_check;
pub mod recent;
pub mod rest;
pub mod ribbon;
pub mod rotary_stack;
// #2772 parts 1-2: the stated rotation-axis resolver (the op's own
// direction, else the machine's stated rotary line, through the WCS).
pub mod rotary_stated;
pub mod schema;
pub mod script;
// #1862: DXF profiles placed on a setup's stock, and the virtual model reference.
pub use camcraft_geometry::sketch_data;
// #1863: the DXF primitives a sketch's data holds, for the viewport's drawing.
pub use camcraft_geometry::dxf;
pub mod sketch;
pub mod sketch_model;
pub mod setup;
pub mod setup_offsets;
// #326: the part's alignment per setup.
pub mod setup_align;
// #343: each setup's own machine and post, and the one-mount law.
pub mod setup_mount;
// #2410: the router options that a machine kind offers (UI and MCP).
pub mod router_offer;
// #870: the WCS origin said in words ("top of stock, front-left corner").
pub mod setup_origin;
// #316: transforms between setups and the setup-relative link.
pub mod setup_relative;
// #1510: a tilted milling WCS is refused at generation.
pub mod setup_wcs_tilt;
// #1720, #1649: a flipped WCS axis is stored; the post mirrors X/Y and refuses the rest.
pub mod setup_wcs_flip;
pub mod setup_post_options;
// #1031: a setup's workholding (chuck/collet) rpm limit.
pub mod setup_workholding;
// #3070: the guided sheet job - stock, Z zero, allowance, clamp plane.
pub mod sheet_job;
pub mod setup_sheet;
// #320: a setup's stock source - its own, or what the setup before leaves.
pub mod setup_stock;
// #406: the incoming stock's sheet for the viewport.
pub mod setup_stock_view;
// #406 (hand check of #519): the incoming stock closed into a block.
pub mod setup_stock_shell;
// #606: the pivot of a typed part-alignment turn.
pub mod setup_turn;
pub use camcraft_sim::solid_rest;
pub use camcraft_sim::sim;
pub use camcraft_geometry::geometry::stock;
pub mod stock_sim;
// #1094: the model that is only the stock's source is not the part.
pub mod stock_source;
pub mod stock_material;
// #1538: fit-to-models measures the parts, never the fixtures.
pub mod stock_fit;
// #1094: the geometry fingerprint a raw-stock-only mark is checked by.
pub mod stock_print;
pub mod stock_export;
// #561: a square stock keeps its allowance round a located part.
pub mod stock_follow;
pub mod stock_play;
pub mod stock_play_worker;
// #1666: the radius the stock sweeps about the lathe's spindle axis.
pub mod stock_turn;
pub mod stock_view;
pub mod tool_library;
pub mod tool_table;
pub mod tool_list;
pub mod tool_list_export;
// #221: the number a tool posts as, and the clash law.
pub mod tool_numbers;
mod tool_reservations;
pub use camcraft_tools::tools;
// #894: the thread mills' form constants, sanity law and pitch door.
pub use camcraft_tools::thread_mill;
// #1444: the undercut cutters' record law and profile.
pub use camcraft_tools::undercut;
// #630: the lathe tool orientation code (LinuxCNC 1-9) and its control point.
pub use camcraft_tools::orientation;
pub use camcraft_tools::catalog_fields;
pub use camcraft_tools::cutting_rows as tool_cutting_rows;
pub use camcraft_tools::waterjet as waterjet_tools;
// #1521: a torch's cut-data recipe per material and plate thickness.
pub use camcraft_tools::plasma_cut as plasma_cut_tools;
pub use camcraft_tools::wire as wire_tools;
// #93: the drawn spun profile of a milling holder.
pub use camcraft_tools::spun;
// #743 part 2: STEP/SpunProfile <-> DXF half-profile conversion.
pub mod tool_profile_dxf;
// #93 slice 2: the drawn cutter law, and the tool's stored drawn cutter.
pub use camcraft_tools::cutter_profile;
// #93 slice 3: the drawn cutter carried onto new numbers, and its neck.
pub use camcraft_tools::cutter_fit;
pub mod tool_cutter;
// #793: the built-in taper holder library.
pub use camcraft_tools::holder_library;
// #368: a tool's modelled body and the reads the judging laws take from it.
pub mod tool_body;
// #371: the editor's placement preview and picks, the sim's posed body.
pub mod tool_body_view;
// #369: lathe-held tool bodies (the turning tool frame).
pub mod tool_body_lathe;
// #369/#396: the turning tool frame and the insert zone, for the UI.
pub use camcraft_tools::{lathe_holder, lathe_insert_zone};
// #361: the one door a body file is read through; the lathe placement reads.
pub mod tool_body_file;
pub mod tool_body_lathe_view;
// #361 / #1040: modular multi-piece tool bodies assembled by ISO 13399 CSYS.
pub mod tool_body_assembly;
// #743: the simulated tool as the collision laws read it, revolved.
pub mod sim_tool_body;
// #1038: a lathe-held body drawn as modelled in the holder law's frame.
pub mod sim_lathe_body;
pub mod units;
pub use camcraft_sim::verify;
// #645: the viewport view preferences (#727's name toggle), per user.
pub mod view;
// #261: the moves a cached path's warnings name.
pub mod warned_moves;
pub mod world;
