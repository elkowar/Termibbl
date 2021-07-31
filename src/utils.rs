use futures_util::future::{AbortHandle, Abortable, Aborted};
use std::future::Future;
use tokio::{
    io::{ReadHalf, WriteHalf},
    net::TcpStream,
    task::JoinHandle,
};
use tokio_util::codec::{FramedRead, FramedWrite};

use crate::encoding::NetworkMessage;

pub struct AbortableTask<T>(AbortHandle, JoinHandle<Result<T, Aborted>>);

impl<T> AbortableTask<T>
where
    T: Send + 'static,
{
    pub fn abort(&self) { self.0.abort() }

    #[allow(dead_code)]
    pub fn abort_handle(&self) -> &AbortHandle { &self.0 }

    #[allow(dead_code)]
    pub fn join_handle(&self) -> &JoinHandle<Result<T, Aborted>> { &self.1 }
}

pub fn dispatch_abortable_task<F>(fut: F) -> AbortableTask<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let join_handle = tokio::spawn(Abortable::new(fut, abort_registration));

    AbortableTask(abort_handle, join_handle)
}

pub type MessageWriter<T> = FramedWrite<WriteHalf<TcpStream>, NetworkMessage<T>>;
pub type MessageReader<T> = FramedRead<ReadHalf<TcpStream>, NetworkMessage<T>>;

pub fn frame_socket<R, W>(st: TcpStream) -> (MessageReader<R>, MessageWriter<W>)
where
    for<'de> R: serde::Deserialize<'de>,
    W: serde::Serialize,
{
    let (r, w) = tokio::io::split(st);
    // let (r, w) = socket.into_split();
    (
        FramedRead::new(r, NetworkMessage::<R>::new()),
        FramedWrite::new(w, NetworkMessage::<W>::new()),
    )
}

pub fn get_time_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
