use tokio::sync::mpsc::error::{SendError, TryRecvError, TrySendError};
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tracing::trace;

pub struct HookedSender<T> {
    id: usize,
    sender: Sender<T>,
}
impl<T> HookedSender<T> {
    pub fn new(buffer: usize, id: usize) -> (Self, Receiver<T>) {
        let (tx, rx) = channel(buffer);
        (HookedSender { id, sender: tx }, rx)
    }
    #[inline]
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        trace!("send hooked (ID:{})", self.id);
        let r = self.sender.send(sender_hooks(value)).await;
        r
    }
    #[inline]
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        trace!("send hooked (ID:{})", self.id);
        let r = self.sender.try_send(sender_hooks(value));
        r
    }
}
pub struct HookedReceiver<T> {
    id: usize,
    receiver: Receiver<T>,
}

impl<T> HookedReceiver<T> {
    pub fn new(buffer: usize, id: usize) -> (Sender<T>, Self) {
        let (tx, rx) = channel(buffer);
        (tx, HookedReceiver { id, receiver: rx })
    }
    #[inline]
    pub async fn recv(&mut self) -> Option<T> {
        let o = self.receiver.recv().await;
        trace!("recv hooked (ID:{})", self.id);
        receiver_hooks(o)
    }

    #[inline]
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let o = self.receiver.try_recv()?;
        trace!("recv hooked (ID:{})", self.id);
        Ok(receiver_hooks(o))
    }
}
#[inline]
fn sender_hooks<T>(value: T) -> T {
    value
}
#[inline]
fn receiver_hooks<T>(value: T) -> T {
    value
}
