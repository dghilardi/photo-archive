mod linux;
mod model;
mod freebsd;

#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "freebsd")]
pub use freebsd::*;