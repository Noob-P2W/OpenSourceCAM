//! Portable CAM model, deterministic toolpaths, and metric G-code postprocessor.
pub mod arcs;
pub mod blank;
pub mod cam;
pub mod collision;
pub mod commands;
pub mod contour;
mod exterior;
pub mod faces;
pub mod features;
pub mod files;
pub mod import;
mod issues;
pub mod machine;
pub mod machine_library;
pub mod machining;
mod material;
pub mod model;
pub mod operation;
pub mod planar;
pub mod post;
pub mod process;
mod roughing;
pub mod session;
mod step_edges;
pub mod stock;
pub mod surface;
pub mod tool_library;
pub mod tool_preview;
pub mod view;
pub mod workholding;
pub mod workspace;

#[cfg(feature = "web")]
pub mod web;

mod contact;

pub mod vise;
