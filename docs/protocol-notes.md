# `herdr-core` (`hc`) Wire Protocol Notes (Spike HC-S0)

> **Verified against:** `/home/tawargy/.local/bin/hc` (`herdr-core` `0.1.21`, OpenRPC `1.3.2`, protocol generation `1`)  
> **Schema fixture:** [`tests/fixtures/openrpc.json`](../tests/fixtures/openrpc.json)  
> **Event fixture:** [`tests/fixtures/events.jsonl`](../tests/fixtures/events.jsonl)

---

## 1. Socket Path Discovery

### Core Default Socket Resolution (Observed on Linux)
- Session name is taken from `HC_SESSION` (defaults to `"default"`).
- Primary runtime directory: `$XDG_RUNTIME_DIR/herdr-core/<session>.sock`  
  Observed on this machine: `/run/user/1000/herdr-core/default.sock` (alongside holder socket `default.holder.<id>.sock`).
- When `XDG_RUNTIME_DIR` is unset on Linux, `hc` still resolves `/run/user/<uid>/herdr-core/<session>.sock` (falling back to `/tmp/herdr-core-<uid>/<session>.sock` if `/run/user/<uid>` is unavailable).
- On macOS, `TMPDIR` or `/tmp/herdr-core-<uid>/<session>.sock` is used as the runtime fallback when `XDG_RUNTIME_DIR` is absent.

### `htui` Client Discovery Precedence
1. CLI flag: `--socket <PATH>`
2. Environment variable: `HTUI_HC_SOCKET`
3. Config file: `config.hc.socket` (`~/.config/herdr-tui/config.toml`)
4. Default core socket path: `$XDG_RUNTIME_DIR/herdr-core/${HC_SESSION:-default}.sock` (or `/run/user/<uid>/herdr-core/${HC_SESSION:-default}.sock`)

---

## 2. Handshake (`hello`) & Mode Negotiation

All connections begin in **plain JSON-RPC 2.0 line-delimited mode** (one JSON object per `\n`-terminated line).

### Plain Handshake Request
```json
{"jsonrpc":"2.0","id":1,"method":"hello","params":{"generation":1}}
```

### Framed Handshake Request
To switch a connection to binary framed multiplexing (used for terminal attach and optionally framed event streams):
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "hello",
  "params": {
    "generation": 1,
    "framed": true,
    "features": ["resize_notifications"]
  }
}
```

### `hello` Response (Always Plain `\n`-Delimited JSON)
Even when `framed: true` is requested, the `hello` response itself is sent as a plain `\n`-terminated JSON-RPC line. **Framed mode begins immediately after the `\n` of the `hello` response** in both directions. Sending plain JSON after `hello {framed: true}` causes the core to close the connection.

Observed `hello` result:
```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "core": { "id": "cc96438561f4a0ecb4308339266f8012", "name": "tawargy" },
    "features": [
      "framed",
      "attach",
      "events",
      "status",
      "shell",
      "resize_notifications",
      "automatic_input",
      "restart_environment"
    ],
    "framed": true,
    "generation": 1,
    "keeps_running_after_logout": true,
    "logout_linger_user": "tawargy",
    "methods": [
      "hello", "server.status", "server.stop", "server.restart",
      "terminal.run", "templates.list", "terminal.list", "terminal.get",
      "terminal.kill", "terminal.rm", "terminal.keep", "terminal.label",
      "terminal.read", "terminal.attach", "terminal.send", "terminal.wait",
      "terminal.keys", "terminal.interrupt", "terminal.put",
      "machine.exec", "file.read", "file.write", "file.edit",
      "doc.get", "doc.set", "events.subscribe",
      "machine.add", "machine.list", "machine.connect", "machine.rename", "machine.rm",
      "rpc.discover"
    ],
    "version": "0.1.21"
  }
}
```

### Unsupported Generation Error
Observed response for `{"generation": 999}`:
```json
{
  "error": {
    "code": -32000,
    "data": {
      "hint": "this core speaks generation 1",
      "kind": "unsupported_generation"
    },
    "message": "protocol generation 999 not supported"
  },
  "id": 2,
  "jsonrpc": "2.0"
}
```

---

## 3. Binary Frame Wire Layout (Framed Mode)

### Frame Header (9 Bytes, Little-Endian)
Every frame in both directions has a fixed 9-byte header followed by `length` payload bytes:

```text
 0               1               2               3
+---------------+---------------+---------------+---------------+
|                      channel (u32, LE)                        |
+---------------+---------------+---------------+---------------+
|   kind (u8)   |               length (u32, LE) ...            |
+---------------+---------------+---------------+---------------+
| ... (1 byte)  |               payload (length bytes) ...      |
+---------------+---------------+---------------+---------------+
```

- `channel` (`u32`, little-endian):
  - `0`: JSON-RPC 2.0 control channel (requests and responses).
  - `>= 1`: Stream channel allocated by `terminal.attach` (`AttachResult.channel`) or `events.subscribe` (`SubscribeResult.channel`).
- `kind` (`u8`): Frame type code.
- `length` (`u32`, little-endian): Byte length of `payload`.

### Empirically Verified Frame Kinds
> **Note:** The numeric codes below were empirically verified against `hc 0.1.21` via MITM proxy and direct socket probes. They differ from earlier draft estimates.

| `kind` | Name | Direction | Payload Layout & Semantics |
|---|---|---|---|
| `0` | `DATA` | Bidirectional | **Channel `0`:** UTF-8 JSON-RPC 2.0 request/response (no trailing `\n` required).<br>**Attach channel (`>= 1`):** Raw PTY I/O bytes (initial paint, then live output; viewer keystrokes client→core).<br>**Events channel (`>= 1`):** `\n`-delimited JSON notification lines. |
| `1` | `CREDIT` | Bidirectional (observed C→S) | `u32` (LE, 4 bytes): Byte count consumed by the receiver to grant flow-control credit (observed `hc attach` sending `CREDIT(259)` after receiving `259` bytes of `DATA`, and `CREDIT(3)` after receiving `3` bytes). |
| `2` | `CLOSE` | Bidirectional | Empty (`0` bytes): Cleanly closes the channel. |
| `3` | `RESIZE` | Bidirectional | `cols: u16` (LE) + `rows: u16` (LE) (4 bytes total).<br>**Client → Core:** Resizes the PTY when attached with `observe: false` (verified via `terminal.get`).<br>**Core → Client:** Sent to observers before replacement paint when both `hello.features` includes `"resize_notifications"` and `terminal.attach` sets `resize_notifications: true` (verified: `6f 00 2c 00` → `111x44` arrived immediately before `277`-byte `DATA` paint). |
| `4` / `5` | `FOCUS` / `FACTS` | Control | Control frames in the `0..=6` range (`terminal.attach` docs: *"RESIZE, FOCUS, FACTS, CREDIT and CLOSE frames control it"*). Accepted without error by the core. |
| `6` | `AUTOMATIC_INPUT` | Client → Core | Programmatic/automated input (per `AttachResult.automatic_input`: *"The entire attach path accepts AUTOMATIC_INPUT (kind 6). Absent means use DATA."*). |

### JSON-RPC in Framed Mode
- Requests are sent on `channel = 0`, `kind = 0` (`DATA`) with the serialized JSON-RPC object as payload (no `\n` needed).
- Responses arrive on `channel = 0`, `kind = 0` (`DATA`).
- Stream data (`terminal.attach` output, `events.subscribe` notifications) arrives concurrently on the assigned `channel >= 1` (`kind = 0`).

---

## 4. `events.subscribe` & Notification Payloads

### Initial Snapshot (RPC Response to `events.subscribe`)
In plain mode, `channel` is `null` and the connection becomes notification-only. In framed mode, `channel` is a `u32` (`>= 1`) carrying JSON-line `DATA` frames (with `CREDIT` flow control).

```json
{
  "id": 2,
  "jsonrpc": "2.0",
  "result": {
    "channel": null,
    "seq": 338,
    "terminals": [
      {
        "address": "t1s",
        "argv": ["/usr/bin/zsh"],
        "cols": 165,
        "created_ms": 1794588626598,
        "cwd": "/home/tawargy",
        "die_with_parent": false,
        "exit_code": 0,
        "finished_ms": 1794593328192,
        "identity": "ghostty",
        "keep": true,
        "labels": { "herdr.space": "..." },
        "name": null,
        "notify": true,
        "parent": null,
        "pid": 35507,
        "reason": "exited",
        "reported": true,
        "rm": false,
        "rows": 45,
        "screen": "exact",
        "signal": null,
        "state": "finished",
        "status": null,
        "status_ms": null,
        "template": null,
        "title": "exit",
        "turn": 22
      }
    ]
  }
}
```

### `event` Notification
```json
{
  "jsonrpc": "2.0",
  "method": "event",
  "params": {
    "seq": 339,
    "terminal": "t28",
    "type": "created",
    "data": { "address": "t28", "state": "live", "..." : "..." }
  }
}
```
Observed `params.type` values in [`tests/fixtures/events.jsonl`](../tests/fixtures/events.jsonl):
- `created` (`data` = full `Terminal` object)
- `title` (`data` = `{"title": "..."}`)
- `labels` (`data` = `{"labels": {...}}`)
- `keep` (`data` = `{"keep": true|false}`)
- `doc` (`terminal` field omitted; `data` = `{"name": "htui.spike_test", "version": 1}`)
- `foreground` (`data` = `{"argv": [...], "cwd": "...", "name": "...", "pid": ...}`)
- `finished` (`data` = `{"exit_code": null|i32, "finished_ms": u64, "reason": "...", "signal": null|i32}`)
- `removed` (`data` = `{}`)
- Additional types documented in OpenRPC `events.subscribe`: `lost`, `restarted`, `status`, `turn`, `size`.

### `heartbeat` Notification (Every 15s)
```json
{
  "jsonrpc": "2.0",
  "method": "heartbeat",
  "params": {
    "seq": 348
  }
}
```

### `missed` Notification
When a subscriber falls behind, the core sends a `missed` notification carrying a fresh snapshot (`seq`, `terminals`). The client must replace its state with the snapshot and ignore any queued events with `seq <= snapshot.seq`.
