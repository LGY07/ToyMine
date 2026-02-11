use tokio::sync::mpsc::error::SendError;
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
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        trace!("send hooked (ID:{})", self.id);
        let r = self.sender.send(sender_hooks(value)).await;
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
    pub async fn recv(&mut self) -> Option<T> {
        let o = self.receiver.recv().await;
        trace!("recv hooked (ID:{})", self.id);
        receiver_hooks(o)
    }
}

fn sender_hooks<T>(value: T) -> T {
    value
}
fn receiver_hooks<T>(value: T) -> T {
    value
}
