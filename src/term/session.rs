use std::time::{Duration, Instant};

use crate::hc::error::HcResult;
use crate::hc::framed::{ChannelHandle, Frame, FrameKind, FramedClient};
use crate::term::screen::{ScreenModel, Vt100Screen};

const RESIZE_DEBOUNCE: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    Connecting,
    Observing,
    Active,
    Ended,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDisposition {
    Sent,
    SuppressedObserving,
    IgnoredNotLive,
}

pub struct AttachSession {
    pub terminal: String,
    pub state: SessionState,
    pub screen: Box<dyn ScreenModel>,
    pub channel: Option<ChannelHandle>,
    pub takeover_hint: bool,
    pub resize_notifications: bool,
    last_resize_sent: Option<Instant>,
    pending_resize: Option<(u16, u16)>,
}

impl AttachSession {
    pub fn new(terminal: String, cols: u16, rows: u16) -> Self {
        Self {
            terminal,
            state: SessionState::Connecting,
            screen: Box::new(Vt100Screen::new(cols, rows, 2000)),
            channel: None,
            takeover_hint: false,
            resize_notifications: false,
            last_resize_sent: None,
            pending_resize: None,
        }
    }

    /// Initial attach always uses `observe: true` (TV-1).
    pub async fn attach_observe(&mut self, client: &FramedClient) -> HcResult<()> {
        let (cols, rows) = self.screen.size();
        let (res, handle) = client
            .attach(&self.terminal, cols, rows, true, true)
            .await?;
        self.resize_notifications = res.resize_notifications;
        self.channel = Some(handle);
        self.state = SessionState::Observing;
        self.takeover_hint = false;
        Ok(())
    }

    /// Attaches directly in Active mode (`observe: false`) for newly spawned terminals.
    pub async fn attach_active(&mut self, client: &FramedClient) -> HcResult<()> {
        if let Some(old) = self.channel.take() {
            let _ = old.close().await;
        }
        let (cols, rows) = self.screen.size();
        let (res, handle) = client
            .attach(&self.terminal, cols, rows, false, true)
            .await?;
        self.resize_notifications = res.resize_notifications;
        self.channel = Some(handle);
        self.state = SessionState::Active;
        self.takeover_hint = false;
        Ok(())
    }

    /// Promotes the session to Active (`observe: false`) (TV-3).
    pub async fn take_over(&mut self, client: &FramedClient) -> HcResult<()> {
        if let Some(old) = self.channel.take() {
            let _ = old.close().await;
        }
        let (cols, rows) = self.screen.size();
        let (res, handle) = client
            .attach(&self.terminal, cols, rows, false, true)
            .await?;
        self.resize_notifications = res.resize_notifications;
        self.channel = Some(handle);
        self.state = SessionState::Active;
        self.takeover_hint = false;
        Ok(())
    }

    /// Demotes the session back to Observing (`observe: true`).
    pub async fn release(&mut self, client: &FramedClient) -> HcResult<()> {
        if let Some(old) = self.channel.take() {
            let _ = old.close().await;
        }
        self.attach_observe(client).await
    }

    pub fn send_input(&mut self, bytes: &[u8]) -> HcResult<InputDisposition> {
        match self.state {
            SessionState::Observing => {
                self.takeover_hint = true;
                Ok(InputDisposition::SuppressedObserving)
            }
            SessionState::Active => {
                if let Some(ch) = &self.channel {
                    ch.send_data(bytes)?;
                    Ok(InputDisposition::Sent)
                } else {
                    Ok(InputDisposition::IgnoredNotLive)
                }
            }
            _ => Ok(InputDisposition::IgnoredNotLive),
        }
    }

    pub fn request_resize(&mut self, cols: u16, rows: u16, now: Instant) -> HcResult<bool> {
        let cols = cols.max(1);
        let rows = rows.max(1);
        self.screen.resize(cols, rows);
        if self.state != SessionState::Active {
            return Ok(false);
        }

        let can_send = match self.last_resize_sent {
            None => true,
            Some(prev) => now.duration_since(prev) >= RESIZE_DEBOUNCE,
        };
        if can_send {
            if let Some(ch) = &self.channel {
                ch.send_resize(cols, rows)?;
                self.last_resize_sent = Some(now);
                self.pending_resize = None;
                return Ok(true);
            }
        } else {
            self.pending_resize = Some((cols, rows));
        }
        Ok(false)
    }

    pub fn flush_pending_resize(&mut self, now: Instant) -> HcResult<bool> {
        let Some((cols, rows)) = self.pending_resize else {
            return Ok(false);
        };
        let ready = match self.last_resize_sent {
            None => true,
            Some(prev) => now.duration_since(prev) >= RESIZE_DEBOUNCE,
        };
        if ready && self.state == SessionState::Active {
            if let Some(ch) = &self.channel {
                ch.send_resize(cols, rows)?;
                self.last_resize_sent = Some(now);
                self.pending_resize = None;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn handle_frame(&mut self, frame: Frame) {
        match frame.kind {
            FrameKind::Data => {
                let len = frame.payload.len() as u32;
                self.screen.process(&frame.payload);
                if len > 0 {
                    if let Some(ch) = &self.channel {
                        let _ = ch.send_credit(len);
                    }
                }
            }
            FrameKind::Resize => {
                if let Some((cols, rows)) = frame.parse_resize() {
                    self.screen.resize(cols, rows);
                }
            }
            FrameKind::Close => {
                self.state = SessionState::Ended;
                self.channel = None;
            }
            _ => {}
        }
    }

    pub async fn recv_next(&mut self) -> Option<Frame> {
        let ch = self.channel.as_mut()?;
        ch.rx.recv().await
    }

    pub fn drain_frames(&mut self) -> bool {
        let mut drained = Vec::new();
        if let Some(ch) = &mut self.channel {
            while let Ok(frame) = ch.rx.try_recv() {
                drained.push(frame);
            }
        }
        let any = !drained.is_empty();
        for frame in drained {
            self.handle_frame(frame);
        }
        any
    }
}
