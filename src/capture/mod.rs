//! Capture subprocess abstraction: request types, slurp geometry parsing,
//! capability probing, and the backend trait.

pub mod backend;
pub mod backends;
pub mod capabilities;
pub mod process;
pub mod request;

pub use backend::CaptureError;
