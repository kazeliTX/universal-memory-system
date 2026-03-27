//! # umms-analyzer
//!
//! Query and content analysis layer:
//! - **EPA**: Embedding Projection Analysis — query vector analysis in tag space
//! - **LGSRR**: Five-layer semantic decomposition of queries
//! - **Intent**: Query intent classification
//! - **Reshaping**: Query vector reshaping using EPA results

pub mod epa;
pub mod intent;
pub mod lgsrr;
pub mod reshaping;
