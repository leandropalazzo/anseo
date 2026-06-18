//! CLI output rendering layer (Story 41.2).
//!
//! Built-in formats (table, JSON) live in the individual command modules.
//! Plugin-provided formats are registered here via [`plugin::PluginOutputFormat`].

pub mod plugin;
