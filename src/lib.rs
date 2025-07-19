#![doc = include_str!("../README.md")]

#[cfg(feature = "async")]
pub mod r#async;
#[cfg(feature = "sync")]
pub mod sync;
