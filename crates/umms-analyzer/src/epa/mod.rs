//! Embedding Projection Analysis (EPA).
//!
//! Analyzes a query vector's position relative to the semantic tag space
//! using K-Means clustering and Power Iteration PCA. Produces metrics
//! (logic depth, cross-domain resonance, semantic axes) that drive
//! dynamic parameter adjustment and query reshaping.

pub mod analyzer;
pub mod kmeans;
pub mod pca;

pub use analyzer::EpaAnalyzer;
pub use kmeans::{Cluster, weighted_kmeans};
pub use pca::power_iteration_pca;
