use crate::{
    data::{UserId, Username},
    events::{EventQueue, EventSender},
    message::{self, InitialRoomState, RoomEvent, ToClient, ToServer},
    server::Message as ServerMessage,
    utils::{self, AbortableTask, MessageReader, MessageWriter},
};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use super::room::RoomMessage;

/// Disconnect client after this seconds of no heartbeat
pub const TIMED_OUT_SECONDS: u64 = 5;

type ClientMessageReader = MessageReader<ToServer>;
type ClientMessageWriter = MessageWriter<ToClient>;

pub type UserSessionInbox = EventSender<Message>;

/// Chat server sends this messages to session
pub enum Message {
    RoomEvent(RoomEvent),
    RoomJoined(EventSender<RoomMessage>, InitialRoomState),
    RoomNotFound,
    Kick(String),
    RoomClosed,
}

#[derive(Clone)]
pub enum UserState {
    // AwaitingVersionCheck
    Idle,
    InQueue {
        username: Username,
    },
    InRoom {
        username: Username,
        key: String,
        room: EventSender<RoomMessage>,
    },
    Stopped,
}

/// `UserSession` actor is responsible for TCP peer communications.
pub struct UserSession {
    /// unique session id
    id: UserId,
    /// socket address
    peer_addr: SocketAddr,
    /// client state
    state: UserState,
    /// this is the event queue for this session
    event_queue: EventQueue<Message>,
    /// this is sender for server event queue
    server: EventSender<ServerMessage>,
    /// Framed sockets
    framed: (ClientMessageReader, ClientMessageWriter),
    /// client must send a message at least once every 5 seconds
    last_hb: Instant,
}

pub struct User {
    pub inbox: UserSessionInbox,
    pub thread_handle: AbortableTask<()>,
}

impl UserSession {
    const NAMES: [&'static str; 4] = ["alice", "bob", "dafny", "spice"];

    pub fn create_user(
        id: UserId,
        peer_addr: SocketAddr,
        server: EventSender<super::Message>,
        client_msg_stream: (ClientMessageReader, ClientMessageWriter),
    ) -> User {
        let session = Self {
            id,
            peer_addr,
            server,
            framed: client_msg_stream,
            event_queue: EventQueue::default(),
            state: UserState::Idle,
            last_hb: Instant::now(),
        };

        User {
            inbox: session.sender().clone(),
            thread_handle: utils::dispatch_abortable_task(session.run()),
        }
    }

    fn generate_name() -> String {
        let mut rng = rand::thread_rng();
        Self::NAMES[rng.gen_range(0, Self::NAMES.len())].to_owned()
    }

    pub fn sender(&self) -> &UserSessionInbox { self.event_queue.sender() }

    fn writer(&mut self) -> &mut ClientMessageWriter { &mut self.framed.1 }

    /// Forward server message to this client
    async fn send(&mut self, msg: ToClient) {
        log::trace!("({}): writing message <> {:?}", self.peer_addr, msg);
        if let Err(err) = self.writer().send(msg).await {
            log::error!("{:?}", err);
            self.stop();
        }
    }

    /// notify room this session is leaving
    fn leave_room(&mut self) {
        if let UserState::InRoom { room, username, .. } = &self.state {
            room.send_with_urgency(RoomMessage::Leave {
                name: username.clone(),
            });
        }
    }

    fn stop(&mut self) {
        self.leave_room();

        self.state = UserState::Stopped;
    }

    async fn kick(&mut self, reason: String) {
        log::debug!("({}): received kick signal <> {}", self.peer_addr, reason);

        self.send(ToClient::Disconnect(reason)).await;
        self.stop()
    }

    async fn on_room_joined(
        &mut self,
        room: EventSender<RoomMessage>,
        mut initial_room_state: InitialRoomState,
    ) {
        if let UserState::InQueue { username } = &self.state {
            if let Some(ref mut game) = initial_room_state.game {
                let this_player_index = game
                    .players
                    .iter()
                    .enumerate()
                    .find(|(_, p)| &p.name == username)
                    .expect("room sent empty player list")
                    .0;

                game.players.swap(0, this_player_index);
            }

            let key = initial_room_state.room.key.clone();
            log::info!("{}", format!("{:?} joined room {}", username, key));

            self.state = UserState::InRoom {
                room,
                key,
                username: username.clone(),
            };

            self.send(ToClient::JoinRoom(initial_room_state)).await;
        } else {
            log::warn!("({}) recv join message without queue", self.peer_addr)
        }
    }

    /// Handle messages from the tcp stream of the client (Client -> Server)
    async fn on_user_msg(&mut self, msg: ToServer) {
        log::debug!("({}): processing message <> {:?}", self.peer_addr, msg);

        match &self.state {
            UserState::Idle => {
                if let ToServer::RequestRoom(maybe_name, req) = msg {
                    let username =
                        Username::new(maybe_name.unwrap_or_else(Self::generate_name), self.id);

                    self.state = UserState::InQueue {
                        username: username.clone(),
                    };

                    self.server.send(ServerMessage::RoomRequest {
                        from: username,
                        req,
                    });
                } else {
                    // TODO: recieved weird messaage from client, is client laggin? maybe disconnect
                }
            }

            UserState::InRoom { room, username, .. } => {
                match msg {
                    ToServer::Chat(chat) => room.send(RoomMessage::Chat {
                        from: username.clone(),
                        msg: chat.into_inner(),
                    }),

                    ToServer::Draw(draw) => room.send(RoomMessage::Draw {
                        from: username.clone(),
                        draw,
                    }),

                    ToServer::RequestRoom(_, _) => {
                        self.kick("You are not allowed to join multiple game rooms.".to_owned())
                            .await
                    }

                    _ => {
                        self.kick("You are being naughty, got a unexpected message.".to_owned())
                            .await
                    }
                };
            }

            _ => (),
        }
    }

    pub async fn run(mut self) {
        log::debug!("started thread for client {}", self.peer_addr);

        struct CheckHeartBeat;
        let timeout_duration = Duration::from_secs(message::HEARTBEAT_INTERVAL + TIMED_OUT_SECONDS);
        let mut hb_check: EventQueue<CheckHeartBeat> = EventQueue::default();

        hb_check
            .sender()
            .send_with_delay(CheckHeartBeat, timeout_duration);

        while !matches!(self.state, UserState::Stopped) {
            let client_msg = self.framed.0.next();
            let server_msg = self.event_queue.recv_async();

            tokio::select! {
                _  = hb_check.recv_async() => {
                    // check client heartbeats, TODO: make heartbeat dependent on `UserState`
                    if Instant::now().duration_since(self.last_hb)
                        > timeout_duration
                    {
                        // heartbeat timed out
                        log::info!(
                            "({}): Client heartbeat failed, disconnecting!",
                            self.peer_addr
                        );

                        let _ = self.writer().send(ToClient::Disconnect("Heartbeat failed".to_owned())).await;
                        break;
                    }
                },

                // Handler for Message, server/room sends this message,
                // if its a `Message::ClientMsg` variant we forward to peer
                Some(msg) = server_msg => {
                    match msg {
                        Message::RoomEvent(msg)=> {
                            if let UserState::InRoom { .. } = &self.state {
                                self.send(ToClient::RoomEvent(msg)).await
                            } else {
                                log::warn!(
                                    "({}) tried to send room event message to client not in room: {:#?}",
                                    self.peer_addr,
                                    msg
                                )
                            }
                        },
                        Message::Kick(reason) => self.kick(reason).await,
                        Message::RoomNotFound => {
                            self.state = UserState::Idle;
                            self.send(ToClient::LeaveRoom(Some("Room not found".to_owned()))).await;
                        }
                        Message::RoomJoined(room_sender, info) => self.on_room_joined(room_sender, info).await,
                        Message::RoomClosed => {
                            self.state = UserState::Idle;
                            self.send(ToClient::LeaveRoom(None)).await;
                        }
                    }
                },

                Some(msg) = client_msg => {
                     match msg {
                         Ok(msg) =>  {
                             match msg {
                                ToServer::Heartbeat => {
                                    self.last_hb = Instant::now();
                                    hb_check.sender().send_with_delay(CheckHeartBeat, timeout_duration);
                                },
                                ToServer::LeaveRoom => self.leave_room(),
                                ToServer::Disconnect => self.stop(),
                                _ => self.on_user_msg(msg).await,
                            };
                         }
                         Err(err) => {
                            log::error!("decode err {:?}", err);
                            break;
                         }
                     }
                }
                else => break,
            }
        }

        // notify server
        self.server.send(ServerMessage::Disconnect(self.id));

        log::debug!("stopped thread for {}.", self.peer_addr,);
    }
}
