#[path = "support/fake_hc.rs"]
mod fake_hc;

use fake_hc::{encode_frame, FakeHcServer, FRAME_DATA};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[tokio::test]
async fn fake_hc_handshake() {
    let srv = FakeHcServer::start().await.expect("start fake server");

    // 1. Plain hello + server.status
    let stream = UnixStream::connect(srv.socket_path())
        .await
        .expect("connect plain");
    let (r, mut w) = stream.into_split();
    let mut reader = BufReader::new(r);

    w.write_all(
        b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"hello\",\"params\":{\"generation\":1}}\n",
    )
    .await
    .unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["result"]["generation"], 1);
    assert_eq!(v["result"]["framed"], false);

    w.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"server.status\",\"params\":{}}\n")
        .await
        .unwrap();
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    let v2: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v2["result"]["version"], "0.1.21");

    // 2. Framed handshake + echo DATA frame
    let fstream = UnixStream::connect(srv.socket_path())
        .await
        .expect("connect framed");
    let (fr, mut fw) = fstream.into_split();
    let mut freader = BufReader::new(fr);
    fw.write_all(
        b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"hello\",\"params\":{\"generation\":1,\"framed\":true}}\n",
    )
    .await
    .unwrap();
    line.clear();
    freader.read_line(&mut line).await.unwrap();
    let hv: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(hv["result"]["framed"], true);

    let attach_req = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "terminal.attach",
        "params": { "terminal": "t1", "cols": 80, "rows": 24, "observe": true }
    }))
    .unwrap();
    fw.write_all(&encode_frame(0, FRAME_DATA, &attach_req))
        .await
        .unwrap();

    let mut hdr = [0u8; 9];
    freader.read_exact(&mut hdr).await.unwrap();
    let ch = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
    let kind = hdr[4];
    let len = u32::from_le_bytes([hdr[5], hdr[6], hdr[7], hdr[8]]) as usize;
    assert_eq!(ch, 0);
    assert_eq!(kind, FRAME_DATA);
    let mut payload = vec![0u8; len];
    freader.read_exact(&mut payload).await.unwrap();
    let av: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    let ach = av["result"]["channel"].as_u64().unwrap() as u32;
    assert!(ach >= 1);

    // Send dummy DATA on attach channel and verify echo
    fw.write_all(&encode_frame(ach, FRAME_DATA, b"ping"))
        .await
        .unwrap();
    freader.read_exact(&mut hdr).await.unwrap();
    let ech = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
    let ekind = hdr[4];
    let elen = u32::from_le_bytes([hdr[5], hdr[6], hdr[7], hdr[8]]) as usize;
    let mut epayload = vec![0u8; elen];
    freader.read_exact(&mut epayload).await.unwrap();
    assert_eq!(ech, ach);
    assert_eq!(ekind, FRAME_DATA);
    assert_eq!(&epayload, b"ping");
}
