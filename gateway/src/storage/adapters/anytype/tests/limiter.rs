use super::mock::Mock;
use crate::storage::adapters::anytype::client::AnytypeClient;

#[tokio::test]
async fn a_rate_limit_shrinks_the_bucket() {
    let mock = Mock::start().await;
    mock.set_rate_limit(1, 1);
    let api = AnytypeClient::with_rate(&mock.url, "k", "space-1", 4.0, 4.0).unwrap();
    let before = api.rate();
    let _error = api.get_object("obj").await.unwrap_err();
    let after = api.rate();
    assert!(after < before);
    assert!((after - 2.0).abs() < f64::EPSILON);
}

#[test]
fn the_default_write_rate_is_inside_the_measured_envelope() {
    let api = AnytypeClient::new("http://127.0.0.1:9", "k", "space-1").unwrap();
    assert!((api.rate() - 0.8).abs() < f64::EPSILON);
}

#[tokio::test]
async fn reads_do_not_wait_on_the_write_bucket() {
    let mock = Mock::start().await;
    mock.seed_objects(1);
    let api = AnytypeClient::with_rate(&mock.url, "k", "space-1", 0.25, 1.0).unwrap();
    let started = std::time::Instant::now();
    for _ in 0..5 {
        api.get_object("obj-1").await.unwrap();
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

#[tokio::test]
async fn a_rate_limit_without_retry_after_reports_no_delay() {
    let mock = Mock::start().await;
    mock.set_rate_limit(1, 0);
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    match api.get_object("missing").await.unwrap_err() {
        crate::storage::adapters::anytype::client::AnytypeError::RateLimited { retry_after } => {
            assert_eq!(retry_after, std::time::Duration::ZERO);
        }
        other => panic!("expected rate limit, got {other:?}"),
    }
}
