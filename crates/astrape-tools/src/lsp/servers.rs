#[derive(Debug, Clone)]
pub struct LspServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub extensions: Vec<String>,
    pub install_hint: String,
}

pub fn get_server_config(language: &str) -> Option<LspServerConfig> {
    match language {
        "typescript" | "javascript" | "typescriptreact" | "javascriptreact" => {
            Some(LspServerConfig {
                name: "typescript-language-server".to_string(),
                command: "typescript-language-server".to_string(),
                args: vec!["--stdio".to_string()],
                extensions: vec![".ts".to_string(), ".tsx".to_string(), ".js".to_string(), ".jsx".to_string()],
                install_hint: "Install with: npm install -g typescript-language-server typescript".to_string(),
            })
        }
        "rust" => Some(LspServerConfig {
            name: "rust-analyzer".to_string(),
            command: "rust-analyzer".to_string(),
            args: vec![],
            extensions: vec![".rs".to_string()],
            install_hint: "Install with: rustup component add rust-analyzer".to_string(),
        }),
        "python" => Some(LspServerConfig {
            name: "pyright".to_string(),
            command: "pyright-langserver".to_string(),
            args: vec!["--stdio".to_string()],
            extensions: vec![".py".to_string()],
            install_hint: "Install with: npm install -g pyright".to_string(),
        }),
        "go" => Some(LspServerConfig {
            name: "gopls".to_string(),
            command: "gopls".to_string(),
            args: vec!["serve".to_string()],
            extensions: vec![".go".to_string()],
            install_hint: "Install with: go install golang.org/x/tools/gopls@latest".to_string(),
        }),
        "c" | "cpp" => Some(LspServerConfig {
            name: "clangd".to_string(),
            command: "clangd".to_string(),
            args: vec![],
            extensions: vec![".c".to_string(), ".cpp".to_string(), ".h".to_string(), ".hpp".to_string()],
            install_hint: "Install with: apt install clangd or brew install llvm".to_string(),
        }),
        "java" => Some(LspServerConfig {
            name: "jdtls".to_string(),
            command: "jdtls".to_string(),
            args: vec![],
            extensions: vec![".java".to_string()],
            install_hint: "Install Eclipse JDT Language Server from eclipse.org".to_string(),
        }),
        _ => None,
    }
}

pub fn known_servers() -> Vec<LspServerConfig> {
    vec![
        get_server_config("typescript").unwrap(),
        get_server_config("rust").unwrap(),
        get_server_config("python").unwrap(),
        get_server_config("go").unwrap(),
        get_server_config("cpp").unwrap(),
        get_server_config("java").unwrap(),
    ]
}
