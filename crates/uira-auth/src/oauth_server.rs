use crate::{AuthError, OAuthCallback, Result};
use actix_web::{web, App, HttpResponse, HttpServer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

type PendingCallbacks = Arc<Mutex<HashMap<String, oneshot::Sender<OAuthCallback>>>>;

pub struct OAuthCallbackServer {
    port: u16,
    pending: PendingCallbacks,
}

impl OAuthCallbackServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(self: Arc<Self>) -> Result<()> {
        let pending = self.pending.clone();

        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(pending.clone()))
                .route("/callback", web::get().to(handle_callback))
                .route("/auth/callback", web::get().to(handle_callback))
        })
        .bind(("127.0.0.1", self.port))
        .map_err(|e| AuthError::Other(format!("Failed to bind server: {}", e)))?
        .run()
        .await
        .map_err(|e| AuthError::Other(format!("Server error: {}", e)))?;

        Ok(())
    }

    pub async fn wait_for_callback(&self, state: &str) -> Result<OAuthCallback> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(state.to_string(), tx);

        let timeout = tokio::time::Duration::from_secs(300);
        tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| AuthError::OAuthFailed("Callback timeout".to_string()))?
            .map_err(|_| AuthError::OAuthFailed("Callback cancelled".to_string()))
    }
}

async fn handle_callback(
    query: web::Query<HashMap<String, String>>,
    data: web::Data<PendingCallbacks>,
) -> HttpResponse {
    let code = query.get("code").cloned().unwrap_or_default();
    let state = query.get("state").cloned().unwrap_or_default();

    if let Some(tx) = data.lock().unwrap().remove(&state) {
        let _ = tx.send(OAuthCallback { code, state });
    }

    HttpResponse::Ok().body(SUCCESS_HTML)
}

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><title>Authorization Successful</title></head>
<body style="font-family: sans-serif; text-align: center; padding: 50px;">
    <h1>✓ Authorization Successful</h1>
    <p>You can close this window and return to the terminal.</p>
</body>
</html>"#;
