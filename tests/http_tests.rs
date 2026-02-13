use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn app() -> axum::Router {
    ctxovrflw::http::routes::router()
}

#[tokio::test]
async fn test_health_endpoint() {
    let response = app()
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "ctxovrflw");
}

#[tokio::test]
async fn test_status_endpoint() {
    let response = app()
        .oneshot(Request::builder().uri("/v1/status").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["service"], "ctxovrflw");
    assert!(json["tier"].is_string());
    assert!(json["memories"].is_number());
}

#[tokio::test]
async fn test_store_and_list_memories() {
    // Store
    let store_response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/memories")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "content": "Test memory from HTTP",
                        "type": "semantic",
                        "tags": ["test"]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(store_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(store_response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["memory"]["id"].is_string());

    // List
    let list_response = app()
        .oneshot(Request::builder().uri("/v1/memories").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(list_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(list_response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
}

#[tokio::test]
async fn test_recall_endpoint() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/memories/recall")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "query": "test",
                        "limit": 5
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["results"].is_array());
}

#[tokio::test]
async fn test_get_nonexistent_memory() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/v1/memories/nonexistent-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn test_delete_nonexistent_memory() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/memories/nonexistent-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], false);
}
