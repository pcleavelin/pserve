#[cfg(not(target_arch = "wasm32"))]
pub mod server;

#[cfg(target_arch = "wasm32")]
pub mod client;

pub mod signal;
pub mod state;

// pub mod htmx;
pub mod dom;

pub mod ui;
