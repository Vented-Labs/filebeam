//! The desktop design kit. Screens are assembled from these pieces so type, colour,
//! shape and motion stay consistent with the web, Android and iOS clients.

pub mod brand;
pub mod button;
pub mod feedback;
pub mod field;
pub mod format;
pub mod motion;
pub mod page;
pub mod progress;
pub mod rail;
pub mod row;
pub mod surface;
pub mod text;

pub use feedback::Tone;
pub use motion::MotionExt;
pub use text::TextExt;
