//! Everything Lemmata needs from the TeX distribution: finding it, telling
//! the user how to install it, and running builds.

pub mod build;
pub mod detect;
mod format;
pub mod os;
pub mod tectonic;

pub use build::{BuildOptions, Outcome, Status, build, build_with, clean};
pub use detect::{Detection, Distribution, detect};
