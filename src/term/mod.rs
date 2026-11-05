//! Everything terminal that is not drawing: PTY sessions, tabs, turning the
//! grid into something drawable, and turning input into bytes.

pub mod frame;
pub mod input;
mod platform;
pub mod session;
pub mod tabs;

pub use platform::{CTRL_C_COPIES_SELECTION, CTRL_V_PASTES};
