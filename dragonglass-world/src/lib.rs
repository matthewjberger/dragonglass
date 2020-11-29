#![warn(
    clippy::all,
    clippy::cognitive_complexity,
    clippy::dbg_macro,
    clippy::expect_used,
    clippy::if_not_else,
    clippy::inefficient_to_string,
    clippy::needless_borrow,
    clippy::todo,
    clippy::too_many_lines,
    clippy::unreachable,
    clippy::unused_self,
    clippy::use_self,
    clippy::wildcard_dependencies,
    clippy::wildcard_imports
)]

pub use self::{gltf::*, world::*};

mod gltf;
mod world;