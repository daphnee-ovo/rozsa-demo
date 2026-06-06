use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub struct CallbackResult {
    pub code: String,
    pub state: String,
}

/// Wait for an OAuth callback on the given port.
/// Parses the first GET request's query parameters and returns them.
/// Responds with a simple HTML success page.
pub async fn wait_for_callback(
    port: u16,
    expected_state: &str,
    cancel: CancellationToken,
) -> Result<CallbackResult, super::types::OAuthLoginError> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| {
            super::types::OAuthLoginError::CallbackServer(format!("failed to bind port {port}: {e}"))
        })?;

    let (mut stream, _) = tokio::select! {
        result = listener.accept() => {
            result.map_err(|e| {
                super::types::OAuthLoginError::CallbackServer(format!("accept failed: {e}"))
            })?
        }
        _ = cancel.cancelled() => {
            return Err(super::types::OAuthLoginError::Cancelled);
        }
    };

    let mut buffer = [0u8; 4096];
    let n = tokio::select! {
        result = stream.read(&mut buffer) => {
            result.map_err(|e| {
                super::types::OAuthLoginError::CallbackServer(format!("read failed: {e}"))
            })?
        }
        _ = cancel.cancelled() => {
            return Err(super::types::OAuthLoginError::Cancelled);
        }
    };

    let request = String::from_utf8_lossy(&buffer[..n]);
    let query = parse_query_from_request(&request).ok_or_else(|| {
        super::types::OAuthLoginError::CallbackServer("invalid request format".to_string())
    })?;

    if let Some(error) = query.get("error") {
        return Err(super::types::OAuthLoginError::Provider(error.clone()));
    }

    let code = query.get("code").cloned().ok_or_else(|| {
        super::types::OAuthLoginError::CallbackServer("missing code parameter".to_string())
    })?;

    let state = query.get("state").cloned().ok_or_else(|| {
        super::types::OAuthLoginError::CallbackServer("missing state parameter".to_string())
    })?;

    if state != expected_state {
        return Err(super::types::OAuthLoginError::CallbackServer(
            "state mismatch".to_string(),
        ));
    }

    let response = "HTTP/1.1 200 OK\r\n\
                    Content-Type: text/html; charset=utf-8\r\n\
                    Connection: close\r\n\
                    \r\n\
                    <!DOCTYPE html>\
                    <html>\
                    <head><title>Login Successful</title></head>\
                    <body>\
                    <h1>Login successful</h1>\
                    <p>You can close this tab.</p>\
                    </body>\
                    </html>";

    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;

    Ok(CallbackResult { code, state })
}

fn parse_query_from_request(request: &str) -> Option<HashMap<String, String>> {
    let first_line = request.lines().next()?;
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let path = parts[1];
    let query_start = path.find('?')?;
    let query_str = &path[query_start + 1..];
    Some(parse_query_string(query_str))
}

fn parse_query_string(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut split = pair.splitn(2, '=');
            let key = split.next()?.to_string();
            let value = split.next()?.to_string();
            Some((key, value))
        })
        .collect()
}
