pub fn to_lsp_position(line_1_indexed: u32, character_0_indexed: u32) -> (u32, u32) {
    (line_1_indexed.saturating_sub(1), character_0_indexed)
}
