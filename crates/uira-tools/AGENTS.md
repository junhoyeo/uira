# uira-tools

LSP client, tool registry, and orchestration utilities.

## LSP Client (`src/lsp/client.rs`)

- `ServerProcess` tracks `opened_documents: HashSet<String>` to avoid sending redundant `textDocument/didOpen` notifications
- `ensure_document_opened()` is called before every LSP operation; checks the set and skips if already opened
- Diagnostics are polled via `publishDiagnostics` notifications with a 2-second timeout
