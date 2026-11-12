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
