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

/// Global fixed-window rate limiter (120 requests/minute).
///
/// Uses a single shared counter rather than per-IP tracking to keep things
/// simple — perfectly adequate for a homelab Mac Mini. Note: because the
/// window resets fully on expiry, up to 2× `max_requests` can occur across
/// a window boundary (e.g. a burst at the end of one window + the start of
/// the next).
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<WindowCounter>>,
    max_requests: u32,
    window_secs: u64,
}

struct WindowCounter {
    count: u32,
    window_start: Instant,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        // Ensure sensible defaults — zero max_requests would deny everything,
        // zero window_secs would disable rate limiting entirely.
        assert!(max_requests > 0, "max_requests must be > 0");
        assert!(window_secs > 0, "window_secs must be > 0");

        Self {
            inner: Arc::new(Mutex::new(WindowCounter {
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

        bucket.count = bucket.count.saturating_add(1);
        bucket.count <= self.max_requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(3, 60);
        assert!(limiter.check().await);
        assert!(limiter.check().await);
        assert!(limiter.check().await);
        assert!(!limiter.check().await); // 4th request denied
    }

    #[tokio::test]
    async fn test_rate_limiter_resets_window() {
        let limiter = RateLimiter::new(2, 1); // 1-second window
        assert!(limiter.check().await);
        assert!(limiter.check().await);
        assert!(!limiter.check().await); // limit hit

        // Wait for window to reset
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        assert!(limiter.check().await); // should be allowed after reset
    }

    #[test]
    #[should_panic(expected = "max_requests must be > 0")]
    fn test_rate_limiter_rejects_zero_max() {
        RateLimiter::new(0, 60);
    }

    #[test]
    #[should_panic(expected = "window_secs must be > 0")]
    fn test_rate_limiter_rejects_zero_window() {
        RateLimiter::new(5, 0);
    }

    #[tokio::test]
    async fn test_rate_limiter_saturating_count() {
        // Verify that count uses saturating arithmetic (won't wrap/panic)
        let limiter = RateLimiter::new(2, 60);
        // Exhaust the limit
        for _ in 0..10 {
            let _ = limiter.check().await;
        }
        // Should still deny, not wrap around
        assert!(!limiter.check().await);
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
