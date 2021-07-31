use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use flume::{select::SelectError, Receiver, Selector, Sender};
use futures_util::{select, FutureExt};

/// simple generic event queue
/// inspired by message.io EventReceiver<E>
pub struct EventQueue<E> {
    sender: EventSender<E>,
    recv: Receiver<E>,
    urgent_recv: Receiver<E>,
    timer_recv: Receiver<(Instant, E)>,
    timers: BTreeMap<Instant, E>,
}

impl<E> Default for EventQueue<E>
where
    E: Send + 'static,
{
    /// create new event queue
    fn default() -> Self {
        let (sender, recv) = flume::unbounded();
        let (immediate_sender, urgent_recv) = flume::unbounded();
        let (timer_sender, timer_recv) = flume::unbounded();

        let sender = EventSender::new(sender, immediate_sender, timer_sender);

        Self {
            recv,
            sender,
            urgent_recv,
            timer_recv,
            timers: BTreeMap::new(),
        }
    }
}

impl<E> EventQueue<E>
where
    E: Send + 'static,
{
    pub fn sender(&self) -> &EventSender<E> { &self.sender }

    fn enque_timers(&mut self) {
        while let Ok((when, event)) = self.timer_recv.try_recv() {
            self.timers.insert(when, event);
        }
    }

    fn selector(&self) -> Selector<E> {
        Selector::new()
            .recv(&self.urgent_recv, |v| v.unwrap())
            .recv(&self.recv, |v| v.unwrap())
    }

    fn next_instant(&self) -> Option<Instant> {
        self.timers.iter().map(|(instant, _)| *instant).next()
    }

    fn next_timed_event(&mut self) -> Option<E> {
        self.next_instant().and_then(|instant| {
            if instant < Instant::now() {
                self.timers.remove(&instant)
            } else {
                None
            }
        })
    }

    // blocking receiver
    pub fn recv(&mut self) -> E {
        self.enque_timers();

        if !self.urgent_recv.is_empty() {
            self.urgent_recv.recv().unwrap()
        } else {
            self.next_timed_event().unwrap_or_else(|| {
                if let Some(next_instant) = self.next_instant() {
                    match self.selector().wait_deadline(next_instant) {
                        Ok(event) => event,
                        Err(SelectError::Timeout) => self.timers.remove(&next_instant).unwrap(),
                    }
                } else {
                    self.selector().wait()
                }
            })
        }
    }

    pub fn recv_timeout(&mut self, timeout: Duration) -> Option<E> {
        self.enque_timers();

        if !self.urgent_recv.is_empty() {
            self.urgent_recv.recv().ok()
        } else {
            self.next_timed_event().or_else(|| {
                if let Some(next_instant) = self.next_instant() {
                    let now = Instant::now();
                    match self
                        .selector()
                        .wait_deadline(std::cmp::min(next_instant, now + timeout))
                    {
                        Ok(event) => Some(event),
                        Err(SelectError::Timeout) => self.next_timed_event(),
                    }
                } else {
                    self.selector().wait_timeout(timeout).ok()
                }
            })
        }
    }

    pub async fn recv_async(&mut self) -> Option<E> {
        self.enque_timers();

        if !self.urgent_recv.is_empty() {
            self.urgent_recv.recv().ok()
        } else if let Some(next_timed_event) = self.next_timed_event() {
            Some(next_timed_event)
        } else if let Some(next_instant) = self.next_instant() {
            select! {
                event = self.urgent_recv.recv_async() => event.ok(),
                event = self.recv.recv_async() => event.ok(),
                _ = tokio::time::delay_until(next_instant.into()).fuse() => self.timers.remove(&next_instant),
            }
        } else {
            select! {
                event = self.urgent_recv.recv_async() => event.ok(),
                event = self.recv.recv_async() => event.ok(),
            }
        }
    }

    // non-blocking
    pub fn try_recv(&mut self) -> Option<E> {
        self.urgent_recv
            .try_recv()
            .or_else(|_| self.next_timed_event().ok_or(()))
            .or_else(|_| self.recv.try_recv())
            .ok()
    }
}

#[derive(Debug)]
pub struct EventSender<E> {
    tx: Sender<E>,
    tx_urgent: Sender<E>,
    tx_timer: Sender<(Instant, E)>,
}

impl<E> EventSender<E>
where
    E: Send + 'static,
{
    fn new(tx: Sender<E>, tx_urgent: Sender<E>, tx_timer: Sender<(Instant, E)>) -> Self {
        Self {
            tx,
            tx_urgent,
            tx_timer,
        }
    }

    pub fn inner(&self) -> &Sender<E> { &self.tx }

    pub fn send(&self, event: E) { self.tx.send(event).ok(); }

    pub fn send_with_urgency(&self, event: E) { self.tx_urgent.send(event).ok(); }

    pub fn send_with_delay(&self, event: E, after: Duration) {
        self.tx_timer.send((Instant::now() + after, event)).ok();
    }
}

impl<E> Clone for EventSender<E>
where
    E: Send + 'static,
{
    fn clone(&self) -> Self {
        EventSender::new(
            self.tx.clone(),
            self.tx_urgent.clone(),
            self.tx_timer.clone(),
        )
    }
}
