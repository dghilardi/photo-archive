mod linux;
pub mod model;
mod freebsd;
pub mod common;

#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "freebsd")]
pub use freebsd::*;