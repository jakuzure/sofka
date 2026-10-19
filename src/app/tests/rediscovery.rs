use super::*;

/// The discovery result of a cluster that has since gained a `widgets` CRD.
fn with_widgets() -> Box<crate::k8s::Discovery> {
    let mut cluster = Cluster::fake();
    cluster.register_kind("example.com", "Widget", "widgets", true);
    Box::new(cluster.fake_discovery())
}

fn deliver(app: &mut App, epoch: u64, result: Result<Box<crate::k8s::Discovery>, String>) {
    app.handle_msg(Msg::Rediscovered { epoch, result });
}

#[tokio::test]
async fn a_kind_missing_at_startup_resolves_after_rediscovery() {
    let (mut app, _rx) = test_app();
    app.user_aliases.insert("wd".into(), "widgets".into());
    app.cluster.add_aliases(&app.user_aliases);
    app.switch_kind("pods");

    palette(&mut app, "widgets");
    assert_eq!(app.kind_plural, "pods");
    assert!(!app.flash_err, "{}", app.flash);
    assert!(app.flash.contains("looking up 'widgets'"), "{}", app.flash);

    let epoch = app.context_epoch;
    deliver(&mut app, epoch, Ok(with_widgets()));
    assert_eq!(app.kind_plural, "widgets");
    assert!(!app.flash_err, "{}", app.flash);

    // The user alias aimed at the new kind now resolves too.
    palette(&mut app, "pods");
    palette(&mut app, "wd");
    assert_eq!(app.kind_plural, "widgets");

    // The palette catalog lists it.
    app.handle_key(press(KeyCode::Char(':'))).unwrap();
    for c in "widg".chars() {
        app.handle_key(press(KeyCode::Char(c))).unwrap();
    }
    assert!(
        app.cmd_suggestions.iter().any(|s| s.label == "widgets"),
        "{:?}",
        app.cmd_suggestions
            .iter()
            .map(|s| &s.label)
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_query_retries_with_its_namespace_after_rediscovery() {
    let (mut app, _rx) = test_app();
    palette(&mut app, "widgets -n prod");
    assert!(!app.flash_err, "{}", app.flash);

    let epoch = app.context_epoch;
    deliver(&mut app, epoch, Ok(with_widgets()));
    assert_eq!(app.kind_plural, "widgets");
    assert_eq!(app.namespace, "prod");
}

#[tokio::test]
async fn a_lookup_that_still_misses_shows_the_usual_error() {
    let (mut app, _rx) = test_app();
    palette(&mut app, "gadgets");
    let epoch = app.context_epoch;
    deliver(&mut app, epoch, Ok(with_widgets()));
    assert!(app.flash_err);
    assert_eq!(app.flash, "No resource matches 'gadgets'");

    // A failed discovery reports the miss as well.
    app.last_rediscovery = None;
    palette(&mut app, "gadgets");
    let epoch = app.context_epoch;
    deliver(&mut app, epoch, Err("connection refused".into()));
    assert!(app.flash_err);
    assert_eq!(app.flash, "No resource matches 'gadgets'");
}

#[tokio::test]
async fn rediscovery_is_throttled_to_one_per_five_seconds() {
    let (mut app, _rx) = test_app();
    palette(&mut app, "widgets");
    assert!(!app.flash_err, "{}", app.flash);

    // A second miss while the first discovery runs reports at once.
    palette(&mut app, "gadgets");
    assert!(app.flash_err);
    assert_eq!(app.flash, "No resource matches 'gadgets'");

    let epoch = app.context_epoch;
    deliver(&mut app, epoch, Ok(with_widgets()));
    assert_eq!(app.kind_plural, "widgets");

    palette(&mut app, "gadgets");
    assert!(app.flash_err);
    assert_eq!(app.flash, "No resource matches 'gadgets'");

    app.last_rediscovery = Some(std::time::Instant::now() - Duration::from_secs(6));
    palette(&mut app, "gadgets");
    assert!(!app.flash_err, "{}", app.flash);
    assert!(app.flash.contains("looking up 'gadgets'"), "{}", app.flash);
}

#[tokio::test]
async fn a_result_from_before_a_context_switch_is_ignored() {
    let (mut app, _rx) = test_app();
    palette(&mut app, "widgets");
    let stale = app.context_epoch;

    let mut west = Cluster::fake();
    west.context = "west".into();
    app.handle_msg(Msg::ContextSwitched {
        generation: app.generation,
        name: "west".into(),
        result: Ok(Box::new(west)),
    });
    let kind = app.kind_plural.clone();

    deliver(&mut app, stale, Ok(with_widgets()));
    assert!(app.cluster.resolve("widgets").is_none());
    assert_eq!(app.kind_plural, kind);
}

#[tokio::test]
async fn navigating_away_keeps_the_new_kinds_but_drops_the_retry() {
    let (mut app, _rx) = test_app();
    palette(&mut app, "widgets");
    palette(&mut app, "deploy");
    assert_eq!(app.kind_plural, "deployments");

    let epoch = app.context_epoch;
    deliver(&mut app, epoch, Ok(with_widgets()));
    assert_eq!(app.kind_plural, "deployments");
    assert!(app.cluster.resolve("widgets").is_some());
}
