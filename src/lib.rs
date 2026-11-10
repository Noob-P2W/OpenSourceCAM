//! Portable CAM model, deterministic toolpaths, and metric G-code postprocessor.
pub mod blank;
pub mod cam;
pub mod collision;
pub mod commands;
pub mod faces;
pub mod features;
pub mod files;
pub mod import;
pub mod machine;
pub mod machine_library;
pub mod machining;
pub mod model;
pub mod operation;
pub mod post;
pub mod process;
mod roughing;
pub mod session;
pub mod stock;
pub mod surface;
pub mod tool_library;
pub mod tool_preview;
pub mod view;
pub mod workholding;
pub mod workspace;

#[cfg(feature = "web")]
pub mod web;
