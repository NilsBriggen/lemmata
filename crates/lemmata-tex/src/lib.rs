//! Everything Lemmata needs from the TeX distribution: finding it, telling
//! the user how to install it, and running builds.

pub mod build;
pub mod detect;
pub mod os;

pub use build::{BuildOptions, Outcome, Status, build};
pub use detect::{Detection, Distribution, detect};
