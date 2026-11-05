//! Everything terminal that is not drawing: PTY sessions, tabs, turning the
//! grid into something drawable, and turning input into bytes.

pub mod frame;
pub mod input;
mod osc;
mod platform;
pub mod session;
pub mod tabs;

pub use platform::{cd_command, quote_path, CTRL_C_COPIES_SELECTION, CTRL_V_PASTES};
