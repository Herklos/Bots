use super::*;

#[test]
pub(super) fn links_switch_roll_back_and_prune() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let p = RuntimePaths::new(home);
    for v in ["1.0.0", "1.1.0", "1.2.0"] {
        std::fs::create_dir_all(p.versions.join(v)).unwrap();
    }
    let v1 = p.versions.join("1.0.0");
    let v2 = p.versions.join("1.1.0");
    assert_eq!(activate(&v1, home).unwrap(), None);
    assert_eq!(activate(&v2, home).unwrap(), Some(v1.clone()));
    assert_eq!(read_link(&p.current), Some(v2.clone()));
    assert_eq!(read_link(&p.previous), Some(v1.clone()));
    assert_eq!(rollback(home).unwrap(), v1);
    assert_eq!(read_link(&p.current), Some(v1));
    assert!(read_link(&p.previous).is_none());
    activate(&v2, home).unwrap();
    let removed = prune(home).unwrap();
    assert_eq!(removed, vec!["1.2.0".to_string()]);
    assert!(p.versions.join("1.0.0").exists() && p.versions.join("1.1.0").exists());
}

#[test]
pub(super) fn rollback_without_a_previous_version_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    assert!(rollback(dir.path()).is_err());
}

#[test]
pub(super) fn installed_runtime_means_the_binary_sits_in_a_version_folder() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let versions = RuntimePaths::new(home).versions;
    std::fs::create_dir_all(versions.join("1.0.0")).unwrap();
    std::fs::write(versions.join("1.0.0").join("hbots"), "").unwrap();
    assert!(installed_runtime(
        &versions.join("1.0.0").join("hbots"),
        home
    ));
    std::fs::write(home.join("hbots"), "").unwrap();
    assert!(!installed_runtime(&home.join("hbots"), home));
}

#[test]
pub(super) fn switching_keeps_the_new_version_when_it_answers() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let p = RuntimePaths::new(home);
    std::fs::create_dir_all(p.versions.join("1.0.0")).unwrap();
    std::fs::create_dir_all(p.versions.join("1.1.0")).unwrap();
    activate(&p.versions.join("1.0.0"), home).unwrap();
    let log = |_: &str| {};
    let restarts = std::cell::Cell::new(0);
    let restart = || {
        restarts.set(restarts.get() + 1);
        Ok(())
    };
    let answering = |v: Option<&str>| v.is_none() || v == Some("1.1.0");
    let s = Switch {
        home,
        managed: true,
        restart: &restart,
        answering: &answering,
        log: &log,
        start_timeout: Duration::from_millis(400),
        rollback_timeout: Duration::from_millis(400),
    };
    switch_to(&p.versions.join("1.1.0"), "1.1.0", &s).unwrap();
    assert_eq!(read_link(&p.current), Some(p.versions.join("1.1.0")));
    assert_eq!(restarts.get(), 1);
}

#[test]
pub(super) fn switching_rolls_back_when_the_new_version_does_not_answer() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let p = RuntimePaths::new(home);
    std::fs::create_dir_all(p.versions.join("1.0.0")).unwrap();
    std::fs::create_dir_all(p.versions.join("1.1.0")).unwrap();
    activate(&p.versions.join("1.0.0"), home).unwrap();
    let log = |_: &str| {};
    let restart = || Ok(());
    let answering = |v: Option<&str>| v != Some("1.1.0");
    let s = Switch {
        home,
        managed: true,
        restart: &restart,
        answering: &answering,
        log: &log,
        start_timeout: Duration::from_millis(400),
        rollback_timeout: Duration::from_millis(400),
    };
    let err = switch_to(&p.versions.join("1.1.0"), "1.1.0", &s).unwrap_err();
    assert!(err.to_string().contains("went back to 1.0.0"), "{err}");
    assert_eq!(read_link(&p.current), Some(p.versions.join("1.0.0")));
}
