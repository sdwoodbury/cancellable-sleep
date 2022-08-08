//! # cancellable-sleep
//! This library allows threads to sleep until a timer elapses or a shutdown signal is received.
//! This is especially useful for daemon processes which may sleep for minutes, where the programmer desires that the sleeping process recieve the signal immediately.
//!
//! Obtain channels via `get_channel`.
//! Perform the "cancellable sleep" via `recv_w_timeout`
//! Create an async task to catch the signals via `await_termination`

use futures::stream::StreamExt;
use signal_hook::consts::signal::*;
use signal_hook_tokio::{Handle, Signals};
use std::io::Error;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::time::{sleep, Duration};

/// Used to catch termination signals and send Quit commands to every registered channel
pub struct SignalHandler {
    signals: Signals,
    handle: Handle,
    channels: Vec<UnboundedSender<ChannelCommand>>,
}

impl Drop for SignalHandler {
    fn drop(&mut self) {
        self.handle.close();
    }
}

impl SignalHandler {
    /// register for SIGHUP, SIGTERM, SIGINT, SIGQUTI
    pub fn init() -> Result<Self, Error> {
        let signals = Signals::new(&[SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;
        let handle = signals.handle();
        Ok(SignalHandler {
            signals,
            handle,
            channels: vec![],
        })
    }

    /// wait for a signal and then send a Quit command to every registered channel
    pub async fn await_termination(&mut self) {
        while let Some(signal) = self.signals.next().await {
            match signal {
                SIGHUP | SIGTERM | SIGINT | SIGQUIT => {
                    for ch in &self.channels {
                        let _ = ch.send(ChannelCommand::Quit);
                    }
                    return;
                }
                _ => unreachable!(),
            }
        }
    }

    /// register a new channel, returning a ChannelHandler, which allows the user to perform a cancellable sleep.
    pub fn get_channel(&mut self) -> ChannelHandler {
        let (tx, rx) = unbounded_channel::<ChannelCommand>();
        self.channels.push(tx);
        ChannelHandler::init(rx)
    }
}

#[derive(PartialEq)]
pub enum ChannelCommand {
    Quit,
    Continue,
}

/// performs the cancellable sleep
pub struct ChannelHandler {
    rx: UnboundedReceiver<ChannelCommand>,
}

impl Drop for ChannelHandler {
    fn drop(&mut self) {
        self.close_rx();
    }
}

impl ChannelHandler {
    pub fn init(rx: UnboundedReceiver<ChannelCommand>) -> Self {
        ChannelHandler { rx }
    }

    fn close_rx(&mut self) {
        self.rx.close();
    }

    /// makes the caller sleep until either `timeout` (in sec) has elapsed or a shutdown signal is received.
    /// returns `ChannelCommand::Continue` upon timeout and `ChannelCommand::Quit` upon cancellation.
    pub async fn recv_w_timeout(&mut self, timeout: u64) -> ChannelCommand {
        // pinning to the heap may be overkill
        let timeout_task = Box::pin(async {
            sleep(Duration::from_secs(timeout)).await;
            ChannelCommand::Continue
        });

        let read_task = Box::pin(self.rx.recv());

        // warning: take care not to poll a future after completion, as the futures here aren't FusedFuture
        // This code shall get the first completed future and then return
        tokio::select! {
            option = read_task => match option {
                Some(cmd) => cmd,
                None => {
                    // recv_w_timeout failed because the channel was closed
                    ChannelCommand::Quit
                }
            },
            cmd = timeout_task => cmd
        }
    }
}
