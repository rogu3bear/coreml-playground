//! Tests for the rate limiter in `server::middleware`.

#![cfg(feature = "ssr")]

use coreml_playground::server::middleware::RateLimiter;

#[tokio::test]
async fn rate_limiter_allows_requests_within_limit() {
    let limiter = RateLimiter::new(5, 60);

    for i in 1..=5 {
        assert!(limiter.check().await, "request {i} of 5 should be allowed");
    }
}

#[tokio::test]
async fn rate_limiter_denies_requests_over_limit() {
    let limiter = RateLimiter::new(3, 60);

    // Exhaust the bucket.
    for _ in 0..3 {
        assert!(limiter.check().await);
    }

    // 4th request should be denied.
    assert!(
        !limiter.check().await,
        "request exceeding max_requests should be denied"
    );
    // 5th also denied.
    assert!(
        !limiter.check().await,
        "subsequent requests should also be denied"
    );
}

#[tokio::test]
async fn rate_limiter_resets_after_window() {
    // RateLimiter uses std::time::Instant which isn't affected by tokio's
    // time manipulation. Use a real 1-second sleep with a 1-second window.
    let limiter = RateLimiter::new(2, 1);

    // Exhaust the bucket.
    assert!(limiter.check().await);
    assert!(limiter.check().await);
    assert!(!limiter.check().await, "should be denied after 2 requests");

    // Sleep past the 1-second window.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // Should be allowed again after the window resets.
    assert!(
        limiter.check().await,
        "should be allowed after window reset"
    );
    assert!(
        limiter.check().await,
        "second request in new window should be allowed"
    );
    assert!(
        !limiter.check().await,
        "third request in new window should be denied"
    );
}
