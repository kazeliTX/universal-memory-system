//! Query vector reshaping via residual pyramid blending.
//!
//! Uses EPA analysis results and the tag co-occurrence network to warp
//! the original query vector toward relevant semantic regions. The
//! reshaping strength is controlled by EPA's alpha parameter.

pub mod reshape;

pub use reshape::QueryReshaper;
