//! # cancellable-sleep
//! This library allows threads to sleep until a timer elapses or a shutdown signal is received.
//! This is especially useful for daemon processes which may sleep for minutes, where the programmer desires that the sleeping process recieve the signal immediately.
//!
//! Obtain channels via `get_channel`.
//! Perform the "cancellable sleep" via `recv_w_timeout`
//! Create an async task to catch the signals via `await_termination`
//!
//! example:
//!
//! ```
//! let mut signal_handler = SignalHandler::init().unwrap();
//! let mut ch1 = signal_handler.get_channel();
//! let mut ch2 = signal_handler.get_channel();
//! let mut ch3 = signal_handler.get_channel();
//!
//! let t1 = || async move {
//!     if ch1.recv_w_timeout(30).await == ChannelCommand::Quit {
//!         println!("quit received by thread 1");
//!     }
//!     println!("thread 1 exiting");
//! };
//!
//! let t2 = || async move {
//!     if ch2.recv_w_timeout(30).await == ChannelCommand::Quit {
//!         println!("quit received by thread 2");
//!     }
//!     println!("thread 2 exiting");
//! };
//!
//! // don't need to pin this because it's not getting passed to join! or select!
//! tokio::spawn(async move {
//!     if ch3.recv_w_timeout(30).await == ChannelCommand::Quit {
//!         println!("quit received by thread 3");
//!     }
//!     println!("thread 3 exiting");
//! });
//!
//! let t1 = Box::pin(t1());
//! let t2 = Box::pin(t2());
//!
//! let t3 = Box::pin(signal_handler.await_termination());
//!
//! let _ = tokio::join!(t1, t2, t3);
//! println!("test1 exiting");
//! ```
use futures::stream::StreamExt;
use signal_hook::consts::signal::*;
use signal_hook_tokio::{Handle, Signals};
use std::io::Error;
use tokio::sync::mpsc::{
    error::TryRecvError, unbounded_channel, UnboundedReceiver, UnboundedSender,
};
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
                    self.channels.iter().for_each(|ch| {
                        // this will fail if the channel has been closed. This is OK.
                        let _ = ch.send(ChannelCommand::Quit);
                    });
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

#[derive(PartialEq, Eq)]
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
            option = read_task => option.unwrap_or(ChannelCommand::Quit),
            cmd = timeout_task => cmd
        }
    }

    /// allows the client to check for the shutdown signal without having to sleep.
    pub fn should_quit(&mut self) -> bool {
        match self.rx.try_recv() {
            Ok(cmd) => cmd == ChannelCommand::Quit,
            Err(e) => e != TryRecvError::Empty,
        }
    }
}
