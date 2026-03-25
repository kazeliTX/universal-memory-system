#![deny(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]

//! # umms-storage
//!
//! Pluggable storage backends for vector, graph, file, and cache layers.

pub mod cache;
pub mod file;
pub mod graph;
pub mod isolation;
pub mod promotion;
pub mod vector;
