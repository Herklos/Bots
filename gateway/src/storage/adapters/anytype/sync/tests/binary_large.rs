use super::super::payload::TEXT_LIMIT;
use super::support::{files_with_path, remote_body, World};

#[tokio::test]
async fn binary_round_trips_through_a_blob() {
    let mut world = World::start("dev-bin").await;
    world.live().await;
    let path = "ada/library/blob.bin";
    let bytes = b"\0bin\xff".to_vec();
    world.write(path, &bytes);
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["properties"]["m_content"], "");
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, bytes);
    assert_eq!(world.read(path).unwrap(), bytes);
}

#[tokio::test]
async fn large_text_round_trips_through_a_blob() {
    let mut world = World::start("dev-large").await;
    world.live().await;
    let path = "ada/library/big.txt";
    let bytes = vec![b'a'; TEXT_LIMIT + 1];
    world.write(path, &bytes);
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["properties"]["m_content"], "");
    assert!(rows[0]["properties"]["m_blob"]
        .as_array()
        .is_some_and(|ids| ids.len() == 1));
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, bytes);
    assert_eq!(world.read(path).unwrap(), bytes);
}
