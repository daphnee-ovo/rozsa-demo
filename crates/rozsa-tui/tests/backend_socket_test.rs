use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

use rozsa_tui::backend::socket::SocketBackend;
use rozsa_tui::backend::{AgentBackend, BackendEvent, Direction};

fn temp_socket_path() -> PathBuf {
    let dir = std::env::temp_dir();
    dir.join(format!("pi_test_socket_{}", std::process::id()))
}

#[tokio::test]
async fn socket_backend_connect_and_receive_state() {
    let path = temp_socket_path();
    let _ = std::fs::remove_file(&path);

    let listener = UnixListener::bind(&path).unwrap();
    let backend = SocketBackend::new(path.to_string_lossy().to_string());
    let mut rx = backend.events();

    // 启动 mock server 线程
    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        // 发送一个 state message
        let state_json = r#"{"type":"state","state":{"appName":"rozsa","version":"0.1.0","cwd":"/tmp","thinkingLevel":"medium","isStreaming":false,"messages":[],"pendingMessages":[],"status":{},"widgetsAbove":{},"widgetsBelow":{},"keybindings":{}}}"#;
        stream.write_all(state_json.as_bytes()).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();

        // 读取客户端发来的 submit 消息
        let reader = BufReader::new(stream);
        let line = reader.lines().next().unwrap().unwrap();
        assert!(line.contains("\"type\":\"submit\""));
        assert!(line.contains("\"text\":\"hello\""));
    });

    backend.connect().await.unwrap();

    // 接收 state 事件
    let event = rx.recv().await.unwrap();
    assert!(matches!(event, BackendEvent::State(s) if s.app_name == "rozsa"));

    // 发送 submit
    backend.submit("hello", vec![]).await.unwrap();

    server_handle.join().unwrap();
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn socket_backend_disconnect_event_on_server_close() {
    let path = temp_socket_path();
    let path2 = path.clone();
    let _ = std::fs::remove_file(&path);

    let listener = UnixListener::bind(&path).unwrap();
    let backend = SocketBackend::new(path.to_string_lossy().to_string());
    let mut rx = backend.events();

    // server 接受连接后立即关闭
    let server_handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        drop(stream);
    });

    backend.connect().await.unwrap();

    // 应该收到 Disconnected
    let event = rx.recv().await.unwrap();
    assert!(matches!(event, BackendEvent::Disconnected));

    server_handle.join().unwrap();
    let _ = std::fs::remove_file(&path2);
}

#[tokio::test]
async fn socket_backend_not_connected_error() {
    let backend = SocketBackend::new("/nonexistent/socket".to_string());
    let _rx = backend.events();

    let result = backend.submit("test", vec![]).await;
    assert!(result.is_err());
}
