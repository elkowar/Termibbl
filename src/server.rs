mod cli;
mod room;
mod session;
mod skribbl;

pub use self::cli::CliOpts;
use self::room::{GameRoom, RoomInbox, RoomMessage};

use crate::{
    data::{GameOpts, UserId, Username},
    events::{EventQueue, EventSender},
    message::RoomRequest,
    utils::{self, AbortableTask},
};
use futures_util::StreamExt;
use rand::{prelude::ThreadRng, Rng};
use session::{User, UserSession};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error")]
    IOError(#[from] std::io::Error),
    #[error("empty optional")]
    EmptyOptional,
}

#[derive(Debug)]
pub enum Message {
    /// Notify server of game request
    RoomRequest {
        from: Username,
        req: RoomRequest,
    },

    /// Notify server of disconnected client.
    Disconnect(UserId),

    /// Notify server of room closing
    RoomClosed(String),

    CtrlC,
}

/// store details about room
struct Room {
    inbox: RoomInbox,
    thread_handle: AbortableTask<()>,
}

pub struct GameServer {
    event_queue: EventQueue<Message>,
    /// hold game rooms by thier key
    rooms: HashMap<String, Room>,
    /// list of words
    words: Arc<Vec<String>>,
    /// holds the default game configuration
    default_game_opts: GameOpts,
    // /// list of players searching for a game
    // game_queue: Vec<UserId>,
    /// holds connected users by id
    connected_users: HashMap<UserId, User>,
    /// random number generator for id & name generation
    rng: ThreadRng,
}

impl GameServer {
    pub fn new(default_game_opts: GameOpts, default_words: Vec<String>) -> Self {
        Self {
            event_queue: EventQueue::default(),
            rooms: HashMap::new(),
            words: Arc::new(default_words),
            default_game_opts,
            // game_queue: Vec::new(),
            connected_users: HashMap::new(),
            rng: rand::thread_rng(),
        }
    }

    pub fn sender(&self) -> &EventSender<Message> { self.event_queue.sender() }

    /// generate unique u8
    fn gen_unique_id(&mut self) -> u8 {
        // garenteed to return if max num of players is 2^8
        loop {
            let id: u8 = self.rng.gen();
            if !self.connected_users.contains_key(&id) {
                return id;
            }
        }
    }

    fn gen_key(&mut self) -> String {
        let rng = &mut self.rng;
        let generator =
            &mut std::iter::repeat(()).map(|_| rng.sample(rand::distributions::Alphanumeric));

        loop {
            let key: String = generator.take(cli::ROOM_KEY_LENGTH).collect();
            if !self.rooms.contains_key(&key) {
                return key;
            }
        }
    }

    fn on_client_disconnect(&mut self, id: UserId) {
        if self.connected_users.remove(&id).is_some() {
            log::info!("#{} left the server", id);
        }
    }

    fn kick_user<S: Into<String>>(&mut self, user_id: UserId, reason: S) {
        if let Some(user) = self.connected_users.remove(&user_id) {
            user.inbox.send(session::Message::Kick(reason.into()));
            log::info!("#{} kicked from the server", user_id);
        }
    }

    fn on_room_close(&mut self, key: String) {
        if key.as_str() == "default" {
            log::info!("did not close main room");
        } else if let Some(_room) = self.rooms.remove(&key) {
            log::info!("closed room {}", key)
        }
    }

    fn dispatch_room(&mut self, key: String, leader: Option<Username>) {
        let mut room = GameRoom::new(key, self.default_game_opts.clone(), &self.words, leader);
        let server = self.sender().clone();
        let sender = room.sender().clone();
        let room_key = room.key().to_owned();

        // dispatch room
        let key = room_key.clone();
        let thread_handle = utils::dispatch_abortable_task(async move {
            if let Err(e) = room.run_loop().await {
                log::error!("room encountered error {}", e);
            }

            // notify server of room death
            server.send_with_urgency(Message::RoomClosed(key));
        });

        self.rooms.insert(
            room_key,
            Room {
                inbox: sender,
                thread_handle,
            },
        );
    }

    fn on_room_request(&mut self, name: Username, action: RoomRequest) {
        let user_id = name.id();
        let inbox = if let Some(user) = self.connected_users.get_mut(&user_id) {
            user.inbox.clone()
        } else {
            return;
        };

        let room_key = match action {
            RoomRequest::Join(room_key) => room_key,
            RoomRequest::Create => {
                let room_key = self.gen_key();
                self.dispatch_room(room_key.clone(), Some(name.clone()));

                room_key
            }
            _ => {
                // TODO: allow users to queue for game rooms
                return self.kick_user(name.id(), "Unimplemented feature".to_owned());
            }
        };

        if let Some(room) = self.rooms.get(&room_key) {
            room.inbox.send(RoomMessage::Join { name, inbox });
        } else {
            inbox.send_with_urgency(session::Message::RoomNotFound);
        }
    }

    /// handle stream of TcpStream's
    fn on_client_connect(&mut self, peer_addr: SocketAddr, st: TcpStream) {
        log::info!("new client connection: {}", peer_addr);

        let unique_id = self.gen_unique_id();
        let sender = self.event_queue.sender().clone();
        let framed_socket_io = utils::frame_socket(st);

        self.connected_users.insert(
            unique_id,
            UserSession::create_user(unique_id, peer_addr, sender, framed_socket_io),
        );
    }

    /// start server listener on given address
    pub async fn listen_on(mut self, addr: &str) -> Result<()> {
        // start tcp listener :: TODO: maybe use udp or both instead?
        let mut tcp_listener = TcpListener::bind(addr)
            .await
            .expect("Could not start webserver (could not bind)")
            .map(|stream| {
                let st = stream.unwrap();
                let addr = st.peer_addr().unwrap();

                st.set_nodelay(true)
                    .expect("Failed to set stream as nonblocking");

                st.set_keepalive(Some(Duration::from_secs(1)))
                    .expect("Failed to set keepalive");

                (st, addr)
            });

        // create default game room for NOW
        self.dispatch_room("default".to_owned(), None);

        loop {
            tokio::select! {
                Some(event) = self.event_queue.recv_async() => {
                    match event {
                        Message::CtrlC => break,
                        Message::RoomRequest { from, req, } => self.on_room_request(from, req),
                        Message::Disconnect (id) => self.on_client_disconnect(id),
                        Message::RoomClosed (key)=> self.on_room_close(key),
                    }
                }

                // listen and accept incoming connections in async thread.
                Some((socket, addr)) = tcp_listener.next() => self.on_client_connect(addr, socket),

                // tcp pipe probably closed, stop server
                else => break,
            };
        }

        log::info!("server closing");

        // disconnect users
        for (_, user) in self.connected_users.drain() {
            user.inbox
                .send_with_urgency(session::Message::Kick("Server Shutdown".into()));
        }

        // close of game rooms
        for (_, room) in self.rooms.drain() {
            room.inbox.send_with_urgency(RoomMessage::Close);
            room.thread_handle.abort(); // dont wait for room to finish
        }

        Ok(())
    }
}
