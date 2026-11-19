use std::time::{Duration, Instant};

use crate::hc::error::HcResult;
use crate::hc::framed::{ChannelHandle, Frame, FrameKind, FramedClient};
use crate::term::screen::{AlacrittyScreen, ScreenModel};

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
    pending_redraw: bool,
}

impl AttachSession {
    pub fn new(terminal: String, cols: u16, rows: u16) -> Self {
        Self {
            terminal,
            state: SessionState::Connecting,
            screen: Box::new(AlacrittyScreen::new(cols, rows, 2000)),
            channel: None,
            takeover_hint: false,
            resize_notifications: false,
            last_resize_sent: None,
            pending_resize: None,
            pending_redraw: false,
        }
    }

    pub fn take_pending_redraw(&mut self) -> bool {
        std::mem::take(&mut self.pending_redraw)
    }

    /// Initial attach always uses `observe: true` (TV-1).
    pub async fn attach_observe(&mut self, client: &FramedClient) -> HcResult<()> {
        let (cols, rows) = self.screen.size();
        let (res, handle) = client
            .attach(&self.terminal, cols, rows, true, true)
            .await?;
        if res.terminal.cols > 0 && res.terminal.rows > 0 {
            self.screen.resize(res.terminal.cols, res.terminal.rows);
        }
        self.resize_notifications = res.resize_notifications;
        self.channel = Some(handle);
        self.state = SessionState::Observing;
        self.takeover_hint = false;
        self.pending_resize = None;
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
        self.pending_resize = None;
        Ok(())
    }

    /// Promotes the session to Active (`observe: false`) at the given dimensions (TV-3).
    pub async fn take_over_at(
        &mut self,
        client: &FramedClient,
        cols: u16,
        rows: u16,
    ) -> HcResult<()> {
        if let Some(old) = self.channel.take() {
            let _ = old.close().await;
        }
        let cols = cols.max(1);
        let rows = rows.max(1);
        let (res, handle) = client
            .attach(&self.terminal, cols, rows, false, true)
            .await?;
        self.screen.resize(cols, rows);
        self.resize_notifications = res.resize_notifications;
        self.channel = Some(handle);
        self.state = SessionState::Active;
        self.takeover_hint = false;
        self.pending_resize = None;
        Ok(())
    }

    /// Promotes the session to Active (`observe: false`) using current screen dimensions (TV-3).
    pub async fn take_over(&mut self, client: &FramedClient) -> HcResult<()> {
        let (cols, rows) = self.screen.size();
        self.take_over_at(client, cols, rows).await
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
        if self.drain_frames() {
            self.pending_redraw = true;
        }
        let cols = cols.max(1);
        let rows = rows.max(1);
        if self.state != SessionState::Active {
            return Ok(false);
        }
        if (cols, rows) == self.screen.size() {
            self.pending_resize = None;
            return Ok(false);
        }

        let can_send = match self.last_resize_sent {
            None => true,
            Some(prev) => now.duration_since(prev) >= RESIZE_DEBOUNCE,
        };
        if can_send {
            if let Some(ch) = &self.channel {
                ch.send_resize(cols, rows)?;
                self.screen.resize(cols, rows);
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
        self.flush_pending_resize_inner(now, false)
    }

    pub fn flush_pending_resize_now(&mut self, now: Instant) -> HcResult<bool> {
        self.flush_pending_resize_inner(now, true)
    }

    fn flush_pending_resize_inner(
        &mut self,
        now: Instant,
        ignore_debounce: bool,
    ) -> HcResult<bool> {
        if self.drain_frames() {
            self.pending_redraw = true;
        }
        let Some((cols, rows)) = self.pending_resize else {
            return Ok(false);
        };
        if self.state != SessionState::Active {
            self.pending_resize = None;
            return Ok(false);
        }
        if (cols, rows) == self.screen.size() {
            self.pending_resize = None;
            return Ok(false);
        }
        let ready = ignore_debounce
            || match self.last_resize_sent {
                None => true,
                Some(prev) => now.duration_since(prev) >= RESIZE_DEBOUNCE,
            };
        if ready {
            if let Some(ch) = &self.channel {
                ch.send_resize(cols, rows)?;
                self.screen.resize(cols, rows);
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
