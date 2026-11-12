//! CamCraft app layer - a clean-room implementation.
//!
//! Provenance: independent implementation specced solely from public
//! documentation of commercial CAM workflows and published technical
//! literature. No HSMWorks source, binaries, or assets were consulted.

pub use camcraft_cl::cl;
pub mod cli;
pub mod clock;
pub mod commands;
pub mod cutting;
// #891: turning cutting data - CSS ops, working diameter, the suggestion.
pub mod cutting_turn;
pub mod smoothing;
pub use camcraft_motion::drilling;
pub mod fixture;
// #339 slice 6: the user fixture library file.
pub mod fixture_library;
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
pub mod model;
// #864: models imported coarser than the importer promises.
pub mod model_fidelity;
pub mod operation;
pub mod pick;
pub use camcraft_post::post;
pub mod path_sig;
pub mod posting;
// #530: the op-family door at post.
mod posting_family;
/// Which spindle a lathe post turns for a section, and the door that
/// keeps a milling operation off the work spindle (#1190).
pub mod lathe_voice;
// #155: standalone Pause / Home entries in the operation list.
pub mod program_entry;
pub use camcraft_motion::plasma;
pub use camcraft_motion::probing;
pub mod project;
// #35: pocket and slot recognition on the model's faces.
pub mod recognize;
// #182: the CPU rasteriser behind the MCP stock render.
pub mod raster;
// #877: the theme and colour-vision mode, kept per user (not per project).
pub mod prefs;
pub mod recent;
pub mod rest;
pub mod ribbon;
pub mod schema;
pub mod script;
pub mod setup;
// #326: the part's alignment per setup.
pub mod setup_align;
// #343: each setup's own machine and post, and the one-mount law.
pub mod setup_mount;
// #316: transforms between setups and the setup-relative link.
pub mod setup_relative;
pub mod setup_post_options;
// #1031: a setup's workholding (chuck/collet) rpm limit.
pub mod setup_workholding;
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
// #1094: the geometry fingerprint a raw-stock-only mark is checked by.
pub mod stock_print;
pub mod stock_export;
// #561: a square stock keeps its allowance round a located part.
pub mod stock_follow;
pub mod stock_play;
pub mod stock_play_worker;
pub mod stock_view;
pub mod tool_library;
pub mod tool_list;
pub mod tool_list_export;
// #221: the number a tool posts as, and the clash law.
pub mod tool_numbers;
pub use camcraft_tools::tools;
// #630: the lathe tool orientation code (LinuxCNC 1-9) and its control point.
pub use camcraft_tools::orientation;
// #93: the drawn spun profile of a milling holder.
pub use camcraft_tools::spun;
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
