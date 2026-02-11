# uira-tools

LSP client, tool registry, and orchestration utilities.

## LSP Client (`src/lsp/client.rs`)

### Document Tracking
- `ServerProcess` tracks `opened_documents: HashMap<String, i32>` (URI -> version)
- `ensure_document_opened()` is called before position-based LSP operations; skips if already opened
- `sync_document()` always reads fresh content and sends `didChange` (or `didOpen` if new)

### Diagnostics
- `diagnostics()` calls `sync_document()` to ensure fresh content before polling
- Sends `textDocument/didChange` with incremented version to trigger server re-analysis
- Polls for `publishDiagnostics` notifications with a 2-second timeout
- Clears cached diagnostics before polling to get fresh results
