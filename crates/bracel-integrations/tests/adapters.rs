#[cfg(feature = "http")]
#[tokio::test]
async fn outbound_bounds_body_and_refuses_redirects_and_unconfigured_origins() {
    use axum::{Router, response::Redirect, routing::get};
    use bracel_integrations::{
        Error,
        http::{HttpMethod, Outbound},
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/ok", get(|| async { "ok" }))
                .route("/large", get(|| async { "123456789" }))
                .route("/redirect", get(|| async { Redirect::temporary("/ok") }))
                .route(
                    "/slow",
                    get(|| async {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        "slow"
                    }),
                ),
        )
        .await
        .unwrap();
    });
    let http = Outbound::new(&[&origin], 8, std::time::Duration::from_millis(100)).unwrap();
    assert_eq!(
        http.request(HttpMethod::GET, &format!("{origin}/ok"), None)
            .await
            .unwrap()
            .body,
        b"ok"
    );
    assert!(matches!(
        http.request(HttpMethod::GET, &format!("{origin}/large"), None)
            .await,
        Err(Error::TooLarge)
    ));
    assert_eq!(
        http.request(HttpMethod::GET, &format!("{origin}/redirect"), None)
            .await
            .unwrap()
            .status,
        307
    );
    assert!(matches!(
        http.request(HttpMethod::GET, &format!("{origin}/slow"), None)
            .await,
        Err(Error::Unavailable)
    ));
    assert!(matches!(
        http.request(HttpMethod::GET, "https://unconfigured.example", None)
            .await,
        Err(Error::InvalidInput)
    ));
    server.abort();
}
#[cfg(feature = "mail")]
#[tokio::test]
async fn mail_capture_escapes_content_and_bounds_capacity() {
    use bracel_integrations::mail::{Mailer, message};
    let capture = Mailer::capture(1).unwrap();
    capture
        .send(
            message(
                "from@example.test",
                "to@example.test",
                "test",
                "<script>&\"'",
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let messages = capture.captured().unwrap();
    let encoded = String::from_utf8(messages[0].formatted()).unwrap();
    assert!(encoded.contains("&lt;script&gt;"));
    assert!(!encoded.contains("<p><script>"));
    assert!(message("bad", "to@example.test", "x", "y").is_err());
    assert!(message("from@example.test", "to@example.test", "x\r\ninjected", "y").is_err());
    assert!(capture.send(messages[0].clone()).await.is_err());
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    assert!(Mailer::local(port).send(messages[0].clone()).await.is_err());
}
#[cfg(feature = "cache")]
#[tokio::test]
async fn cache_scopes_coalesces_invalidates_and_expires() {
    use bracel_integrations::{Error, cache::ScopedCache};
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    let cache = ScopedCache::new(4096, Duration::from_millis(50), 128).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let loader = || {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(5)).await;
            Ok(b"one".to_vec())
        }
    };
    let (a, b) = tokio::join!(
        cache.get_or_load("owner", "id", loader()),
        cache.get_or_load("owner", "id", loader())
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(
        cache
            .get_or_load("other", "id", async { Ok(b"other".to_vec()) })
            .await
            .unwrap(),
        b"other"
    );
    cache.invalidate("owner", "id").await;
    cache.get_or_load("owner", "id", loader()).await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
    tokio::time::sleep(Duration::from_millis(70)).await;
    cache.get_or_load("owner", "id", loader()).await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 3);
    assert!(
        cache
            .get_or_load("owner", "big", async { Ok(vec![0; 129]) })
            .await
            .is_err()
    );
    assert!(
        cache
            .get_or_load("owner", "fail", async { Err(Error::Unavailable) })
            .await
            .is_err()
    );
}

#[cfg(feature = "mail")]
#[tokio::test]
async fn smtp_acceptance_and_permanent_failure_are_classified() {
    use bracel_integrations::{
        Error,
        mail::{Mailer, message},
    };
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    for reject in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = socket.into_split();
            let mut reader = BufReader::new(reader);
            writer
                .write_all(b"220 localhost test SMTP\r\n")
                .await
                .unwrap();
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).await.unwrap() == 0 {
                    break;
                }
                let reply = if line.starts_with("EHLO") || line.starts_with("MAIL") {
                    "250 OK\r\n"
                } else if line.starts_with("RCPT") {
                    if reject {
                        "550 rejected\r\n"
                    } else {
                        "250 OK\r\n"
                    }
                } else if line.starts_with("DATA") {
                    writer.write_all(b"354 end with dot\r\n").await.unwrap();
                    let mut bytes = 0;
                    loop {
                        line.clear();
                        let n = reader.read_line(&mut line).await.unwrap();
                        if n == 0 || line == ".\r\n" {
                            break;
                        }
                        bytes += n;
                    }
                    assert!(bytes > 0);
                    "250 accepted\r\n"
                } else if line.starts_with("QUIT") {
                    "221 bye\r\n"
                } else {
                    "250 OK\r\n"
                };
                if writer.write_all(reply.as_bytes()).await.is_err() {
                    break;
                }
            }
        });
        let result = Mailer::local(port)
            .send(message("from@example.test", "to@example.test", "test", "hello").unwrap())
            .await;
        if reject {
            assert_eq!(result.unwrap_err(), Error::Rejected);
        } else {
            result.unwrap();
        }
        server.abort();
    }
}
#[cfg(feature = "storage")]
#[tokio::test]
async fn storage_scope_limits_and_missing_objects() {
    use bracel_integrations::{Error, storage::Storage};
    let root = std::env::temp_dir().join(format!("bracel-storage-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for store in [
        Storage::memory(8).unwrap(),
        Storage::local(&root, 8).unwrap(),
    ] {
        let id = store.put("owner", b"hello".to_vec()).await.unwrap();
        assert_eq!(store.get("owner", id).await.unwrap(), b"hello");
        assert_eq!(store.get("other", id).await.unwrap_err(), Error::NotFound);
        assert!(store.put("owner", vec![0; 9]).await.is_err());
        store.delete("owner", id).await.unwrap();
        assert_eq!(store.get("owner", id).await.unwrap_err(), Error::NotFound);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(feature = "telemetry")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn telemetry_exports_to_local_collector_and_bounds_shutdown() {
    use bracel_integrations::telemetry::{
        Telemetry,
        opentelemetry::trace::{Span, Tracer},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let count = Arc::new(AtomicUsize::new(0));
    let received = count.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/traces", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route(
                "/v1/traces",
                axum::routing::post(move |body: axum::body::Bytes| {
                    let received = received.clone();
                    async move {
                        assert!(!body.is_empty());
                        received.fetch_add(1, Ordering::SeqCst);
                        axum::http::StatusCode::OK
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let telemetry = tokio::task::spawn_blocking(move || {
        Telemetry::otlp("bracel-test", &endpoint, 1.0).unwrap()
    })
    .await
    .unwrap();
    telemetry.tracer().start("test.operation").end();
    tokio::task::spawn_blocking(move || telemetry.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(count.load(Ordering::SeqCst) > 0);
    server.abort();
    let telemetry = tokio::task::spawn_blocking(|| {
        Telemetry::otlp("bracel-test", "http://127.0.0.1:1/v1/traces", 1.0).unwrap()
    })
    .await
    .unwrap();
    telemetry.tracer().start("unavailable.collector").end();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::task::spawn_blocking(move || {
            let _ = telemetry.shutdown();
        }),
    )
    .await
    .unwrap()
    .unwrap();
}
