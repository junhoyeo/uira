//! Output formatting for comment detection results.

mod formatter;
mod xml_builder;

pub use formatter::format_hook_message;
pub use xml_builder::build_comments_xml;
