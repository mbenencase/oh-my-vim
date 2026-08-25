//! An LSP client that keeps async out of the editor.
//!
//! Each server runs on its own thread with a current-thread tokio runtime.
//! The editor sends [`Request`]/[`Notify_`] values in and drains [`Event`]s from
//! a plain `std::sync::mpsc` channel, so nothing in the render loop is async.

pub mod client;
pub mod protocol;
pub mod registry;
pub mod uri;

pub use client::{Client, Event, LspPosition, Notify_ as Notification, Request};
pub use lsp_types;
pub use registry::{Registry, ServerSpec};
pub use uri::{path_to_uri, uri_to_path};
