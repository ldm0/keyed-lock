//! This crate provides a keyed lock, which allows you to lock a resource based on a key. This is useful when you have a collection of resources and you want to lock individual resources by their key.
//! 
//! This crate provides both synchronous and asynchronous implementations.

#[cfg(feature = "async")]
pub mod r#async;
#[cfg(feature = "sync")]
pub mod sync;
