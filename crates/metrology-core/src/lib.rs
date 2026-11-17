//! UI- and camera-independent measurement core.

pub mod diameter;
pub mod field;
pub mod illumination;
pub mod image;
pub mod lattice;
pub mod linalg;
pub mod periodic;
pub mod png;
pub mod profile;
pub mod spline;
pub mod thread_flank;
pub mod thread_form;
pub mod thread_measure;
pub mod thread_standard;
pub mod thread_teeth;
pub mod tolerance;
pub mod verdict;

#[cfg(test)]
pub(crate) mod testutil;
#[cfg(test)]
pub(crate) mod thread_synth;
