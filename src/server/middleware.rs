use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Optional API key guard. Only enforced when COREML_API_KEY is set.
pub async fn api_key_guard(request: Request, next: Next) -> Result<Response, StatusCode> {
    let expected_key = match std::env::var("COREML_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => return Ok(next.run(request).await),
    };

    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if auth_header == format!("Bearer {}", expected_key) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Global token-bucket rate limiter (120 requests/minute).
///
/// Uses a single shared bucket rather than per-IP tracking to keep things
/// simple — perfectly adequate for a homelab Mac Mini.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<TokenBucket>>,
    max_requests: u32,
    window_secs: u64,
}

struct TokenBucket {
    count: u32,
    window_start: Instant,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TokenBucket {
                count: 0,
                window_start: Instant::now(),
            })),
            max_requests,
            window_secs,
        }
    }

    /// Check whether the next request is allowed. Returns `true` if within
    /// limits, `false` if the caller should be throttled.
    pub async fn check(&self) -> bool {
        let mut bucket = self.inner.lock().await;
        let now = Instant::now();

        if now.duration_since(bucket.window_start).as_secs() >= self.window_secs {
            bucket.count = 0;
            bucket.window_start = now;
        }

        bucket.count += 1;
        bucket.count <= self.max_requests
    }
}

/// Axum middleware that enforces a global rate limit.
///
/// Requires the `RateLimiter` to be stored as an `Extension` on the router.
pub async fn rate_limit(
    axum::extract::Extension(limiter): axum::extract::Extension<RateLimiter>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if limiter.check().await {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}
