use papaproc::readiness::{check_http_once, check_tcp_once, parse_host_port};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn tcp_probe_succeeds_when_port_accepts_connections() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = listener.accept().await;
    });

    check_tcp_once(&addr.to_string()).await.unwrap();
}

#[tokio::test]
async fn http_probe_succeeds_on_2xx_response() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 1024];
        let _ = socket.read(&mut buffer).await.unwrap();
        socket
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
    });

    check_http_once(&format!("http://{addr}/health"))
        .await
        .unwrap();
}

#[test]
fn extracts_host_port_from_urls_and_tcp_targets() {
    assert_eq!(
        parse_host_port("localhost:5432").unwrap(),
        ("localhost".to_string(), 5432)
    );
    assert_eq!(
        parse_host_port("http://localhost:8080/health").unwrap(),
        ("localhost".to_string(), 8080)
    );
}
