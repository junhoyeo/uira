use crate::widgets::dialog::DialogAlert;

pub fn dialog_status(mcp: &str, lsp: &str, formatters: &str) -> DialogAlert {
    let message = format!("MCP: {}\nLSP: {}\nFormatters: {}", mcp, lsp, formatters);
    DialogAlert::new("Status", message)
}
