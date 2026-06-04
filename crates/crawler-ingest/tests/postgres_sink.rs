use chrono::{Duration, Utc};
use opengeo_core::ProjectId;
use opengeo_crawler_ingest::{
    BotRangeVerifier, CidrRange, IngestSink, MetricsParams, MetricsStore, PostgresCrawlerSink,
    PrivacyMode, RawCrawlerHit,
};
use sqlx::PgPool;

#[cfg_attr(
    not(feature = "live_db_tests"),
    ignore = "requires DATABASE_URL and --features live_db_tests"
)]
#[sqlx::test(migrations = "../storage/migrations")]
async fn postgres_sink_is_idempotent_and_metrics_are_verified_only(pool: PgPool) {
    let project_id = ProjectId::new();
    let project_uuid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
    sqlx::query("INSERT INTO projects (id, name) VALUES ($1, $2)")
        .bind(project_uuid)
        .bind("crawler-test")
        .execute(&pool)
        .await
        .unwrap();

    let verifier = BotRangeVerifier::from_ranges(vec![(
        "openai-gptbot".into(),
        CidrRange::parse("203.0.113.0/24").unwrap(),
    )]);
    let raw = RawCrawlerHit {
        raw_event_id: "nginx:1".into(),
        ts: Utc::now() - Duration::hours(1),
        user_agent: "GPTBot".into(),
        path: "/docs".into(),
        status: 200,
        source_adapter: "nginx".into(),
        client_ip: Some("203.0.113.7".into()),
        region: Some("NL".into()),
    };
    let event = raw
        .clone()
        .normalize(
            project_id,
            PrivacyMode::Hashed,
            "test-salt",
            verifier.verify_user_agent_ip(&raw.user_agent, raw.client_ip.as_deref().unwrap()),
        )
        .unwrap();
    let sink = PostgresCrawlerSink::new(pool.clone());
    assert_eq!(
        sink.insert_events(std::slice::from_ref(&event))
            .await
            .unwrap(),
        1
    );
    assert_eq!(sink.insert_events(&[event]).await.unwrap(), 0);

    let spoofed = RawCrawlerHit {
        raw_event_id: "nginx:2".into(),
        ts: Utc::now() - Duration::minutes(30),
        user_agent: "GPTBot".into(),
        path: "/private".into(),
        status: 404,
        source_adapter: "nginx".into(),
        client_ip: Some("198.51.100.7".into()),
        region: None,
    };
    let event = spoofed
        .clone()
        .normalize(
            project_id,
            PrivacyMode::Hashed,
            "test-salt",
            verifier
                .verify_user_agent_ip(&spoofed.user_agent, spoofed.client_ip.as_deref().unwrap()),
        )
        .unwrap();
    assert_eq!(sink.insert_events(&[event]).await.unwrap(), 1);

    let metrics = MetricsStore::new(pool.clone())
        .fetch(MetricsParams {
            project_id,
            days: 7,
            include_unverified: false,
        })
        .await
        .unwrap();
    assert_eq!(metrics.bots[0].hits, 1);
    assert!(metrics.error_paths.is_empty());

    let metrics_with_quarantine = MetricsStore::new(pool)
        .fetch(MetricsParams {
            project_id,
            days: 7,
            include_unverified: true,
        })
        .await
        .unwrap();
    assert_eq!(metrics_with_quarantine.bots[0].hits, 2);
    assert_eq!(metrics_with_quarantine.error_paths[0].path, "/private");
}
