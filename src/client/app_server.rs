use std::{fmt::Debug, net::SocketAddr, time::Duration};

use futures_util::{sink::Sink, FutureExt, SinkExt, StreamExt, TryFutureExt};
use tokio::net::TcpStream;

use crate::{
    encoding,
    events::{EventQueue, EventSender},
    message,
    utils::{self, AbortableTask},
};

use super::{
    app::Event,
    error::{Error, Result},
};

#[derive(Debug, Copy, Clone)]
pub enum ConnectionStatus {
    NotConnected,
    Connecting,
    Connected,
    NotFound,
    Dropped,
    Timedout,
}

impl Default for ConnectionStatus {
    fn default() -> Self { Self::NotConnected }
}

pub enum NetEvent {
    SessionCreate(ServerSession),
    Status(ConnectionStatus),
    Message(Box<message::ToClient>),
}

pub struct ServerSession {
    server_addr: SocketAddr,
    server_msg_tx: EventSender<message::ToServer>,
}

#[derive(Default)]
pub struct AppServer {
    session: Option<ServerSession>,
    connection_status: ConnectionStatus,
    connection_attempt_task: Option<AbortableTask<()>>,
}

impl AppServer {
    pub fn is_connected(&self) -> bool {
        self.session.is_some() && matches!(self.connection_status, ConnectionStatus::Connected)
    }

    pub fn connection_status(&self) -> ConnectionStatus {
        if self.connection_attempt_task.is_some() {
            ConnectionStatus::Connecting
        } else {
            self.connection_status
        }
    }

    pub fn send_message(&self, message: message::ToServer) {
        if let Some(ref session) = self.session {
            // TODO: check if disconnected
            session.send_server_msg(message);
        }
    }

    pub fn addr(&self) -> Option<String> {
        self.session.as_ref().map(|s| s.server_addr.to_string())
    }

    pub fn set_status(&mut self, status: ConnectionStatus) {
        if !matches!(status, ConnectionStatus::Connected) {
            self.disconnect()
        }

        self.connection_status = status;
    }

    pub(crate) fn set_session(&mut self, session: ServerSession) -> Result<()> {
        self.connection_status = ConnectionStatus::Connected;
        self.session = Some(session);

        let _ = self.connection_attempt_task.take();

        Ok(())
    }

    pub fn disconnect(&mut self) {
        if let Some(attempting_connection_handle) = self.connection_attempt_task.take() {
            attempting_connection_handle.abort();
        }

        let _ = self.session.take();
        self.connection_status = ConnectionStatus::NotConnected;
    }

    /// attempt to connect to termibbl server
    pub fn connect(&mut self, server_addr: SocketAddr, app_tx: EventSender<Event>) {
        if self.is_connected() {
            self.disconnect();
        }

        let handle = TcpStream::connect(server_addr)
            .map_ok(|socket| {
                // TODO: verify this is a Termibbl server and versions are compatible
                utils::frame_socket(socket)
            })
            .map_err(Error::from)
            .map(move |result| {
                let net_event = match result {
                    // create session to handle this socket and notify server
                    Ok(socket) => NetEvent::SessionCreate(ServerSession::create(
                        server_addr,
                        app_tx.clone(),
                        socket.0,
                        socket.1,
                    )),

                    Err(err) => {
                        let status = match err {
                            Error::SendError(_) => ConnectionStatus::NotConnected,
                            Error::IOError(err) => match err.kind() {
                                std::io::ErrorKind::TimedOut => ConnectionStatus::Timedout,
                                _ => ConnectionStatus::NotFound,
                            },
                            _ => unreachable!(),
                        };

                        NetEvent::Status(status)
                    }
                };

                app_tx.send(Event::Net(net_event));
            });

        self.connection_attempt_task
            .replace(utils::dispatch_abortable_task(handle));
    }
}

impl Drop for ServerSession {
    fn drop(&mut self) {
        self.server_msg_tx
            .send_with_urgency(message::ToServer::Disconnect);
    }
}

impl ServerSession {
    fn send_server_msg(&self, message: message::ToServer) { self.server_msg_tx.send(message) }

    fn create<
        S: StreamExt<Item = encoding::Result<message::ToClient>> + Unpin + Send + 'static,
        W: Sink<message::ToServer> + Unpin + Send + 'static,
    >(
        server_addr: SocketAddr,
        app_tx: EventSender<Event>,
        mut server_to_client: S,
        mut client_to_server: W,
    ) -> Self
    where
        W::Error: Debug,
    {
        let mut event_queue = EventQueue::<message::ToServer>::default();
        let server_msg_tx = event_queue.sender().clone();

        let connection_loop = async move {
            // send heartbeats every couple seconds otherwise server will disconnect
            let mut heartbeat =
                tokio::time::interval(Duration::from_secs(message::HEARTBEAT_INTERVAL));

            let connection_status = loop {
                tokio::select! {
                    _ = heartbeat.tick() => event_queue.sender().send(message::ToServer::Heartbeat),

                    Some(to_server_msg) = event_queue.recv_async() => {
                        if let message::ToServer::Disconnect = to_server_msg {
                            let _ = client_to_server.send(to_server_msg).await;
                            break ConnectionStatus::NotConnected;
                        } else if client_to_server.send(to_server_msg).await.is_err() {
                            break ConnectionStatus::Dropped;
                        }
                    }

                    Some(server_msg) = server_to_client.next() => {
                        if let Ok(msg) = server_msg {
                            match msg {
                                message::ToClient::Disconnect(_) => break ConnectionStatus::Dropped,
                                _ => app_tx.send(Event::Net(NetEvent::Message(Box::new(msg))))
                            }
                        } else {
                            break ConnectionStatus::Dropped;
                        }
                    }

                    else => break ConnectionStatus::NotConnected,
                };
            };

            app_tx.send(Event::Net(NetEvent::Status(connection_status)));
        };

        tokio::spawn(connection_loop);

        Self {
            server_addr,
            server_msg_tx,
        }
    }
}
