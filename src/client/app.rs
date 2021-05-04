use std::{net::SocketAddr, time::Duration};

use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, EventStream, KeyCode, KeyEvent, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use tui::Terminal;

use crate::{
    data::Username,
    events::{EventQueue, EventSender},
    message::ToClient,
    utils::{self, AbortableTask},
};

use super::{
    app_server::{AppServer, NetEvent},
    error::Result,
    ui::{self, Room, StartMenu, View},
    InputEvent,
};

pub enum Event {
    Input(InputEvent),
    Net(NetEvent),
    CtrlC,
    CloseNotification,
}

enum State {
    Start(StartMenu),
    InGameRoom(Box<Room>),
}

impl Default for State {
    fn default() -> Self { Self::Start(StartMenu::default()) }
}

impl From<Room> for State {
    fn from(v: Room) -> Self { Self::InGameRoom(Box::new(v)) }
}

#[derive(Default)]
pub struct App {
    event_queue: EventQueue<Event>,
    state: State,
    server: AppServer,
    should_exit: bool,
    notifications: Vec<String>,
}

impl App {
    pub fn sender(&self) -> &EventSender<Event> { self.event_queue.sender() }

    pub fn server(&self) -> &AppServer { &self.server }

    pub fn exit(&mut self) { self.should_exit = true; }

    pub fn set_name_input(&mut self, name: String) {
        if let State::Start(start_menu) = &mut self.state {
            start_menu.username_input.set_content(name);
        }
    }

    pub fn set_host_input(&mut self, addr: String) {
        if let State::Start(start_menu) = &mut self.state {
            start_menu.host_input.set_content(addr);
        }
    }

    pub fn get_current_view(&self) -> &dyn View {
        match &self.state {
            State::Start(start_menu) => start_menu,
            State::InGameRoom(room) => room.get_current_view(),
        }
    }

    fn get_current_view_mut(&mut self) -> &mut dyn View {
        match &mut self.state {
            State::Start(start_menu) => start_menu,
            State::InGameRoom(room) => room.get_current_view_mut(),
        }
    }

    pub fn connect_to_server(&mut self, addr: SocketAddr) {
        self.server.connect(addr, self.event_queue.sender().clone());
    }

    pub fn username(&self) -> Option<&Username> {
        match &self.state {
            State::InGameRoom(room) => Some(&room.username),
            _ => None,
        }
    }

    fn go_back(&mut self) {
        match &mut self.state {
            State::InGameRoom(_) => {
                self.server
                    .send_message(crate::message::ToServer::LeaveRoom);
            }

            State::Start(menu) => {
                if menu.host_input.has_focus() {
                    self.exit();
                } else {
                    self.server.disconnect();
                }
            }
        };
    }

    fn display_notif(&mut self, error: String) {
        self.notifications.push(error);
        self.sender()
            .send_with_delay(Event::CloseNotification, Duration::from_secs(4));
    }

    fn handle_net_event(&mut self, event: NetEvent) -> Result<()> {
        match event {
            NetEvent::SessionCreate(session) => {
                self.server.set_session(session)?;

                let is_connected = self.server.is_connected();
                if let State::Start(state) = &mut self.state {
                    state.on_connection_status_changed(is_connected);
                }
            }

            NetEvent::Status(status) => {
                let addr = self.server.addr();
                self.server.set_status(status);

                let is_connected = self.server.is_connected();
                match &mut self.state {
                    State::InGameRoom(ref room) => {
                        if !is_connected {
                            self.state =
                                State::Start(StartMenu::new(addr, Some(room.username.to_string())));
                        }
                    }

                    State::Start(ref mut start_menu) => {
                        start_menu.on_connection_status_changed(is_connected);
                    }
                };
            }

            NetEvent::Message(message) => {
                if let ToClient::Disconnect(reason) = *message {
                    // if server disconnects us
                    self.display_notif(reason);
                } else if let State::InGameRoom(room) = &mut self.state {
                    match *message {
                        ToClient::RoomEvent(event) => room.process_event(event),
                        ToClient::LeaveRoom(maybe_reason) => {
                            // kick to start screen
                            self.state = State::Start(StartMenu::new(
                                self.server.addr(),
                                Some(room.username.to_string()),
                            ));

                            if let Some(reason) = maybe_reason {
                                self.display_notif(reason)
                            }
                        }

                        _ => {
                            // server sent unknown message ... do nothing
                            // panic!(
                            // "server & client state not in sync {:?}",
                            // message
                            // ),
                        }
                    };
                } else if let ToClient::JoinRoom(initial_room_state) = *message {
                    self.state = Room::new(initial_room_state).into();
                } else {
                    unimplemented!("msg {:#?}", *message)
                }
            }
        }

        Ok(())
    }

    fn handle_input_event(&mut self, event: InputEvent) -> Result<()> {
        if event == InputEvent::Key(KeyCode::Esc.into()) {
            self.go_back();
        } else {
            let view = self.get_current_view_mut();
            let action = match event {
                InputEvent::Key(key_event) => view.on_key_event(key_event),
                InputEvent::Mouse(mouse_event) => view.on_mouse_event(mouse_event),
                InputEvent::Resize(x, y) => {
                    view.on_resize((x, y));
                    return Ok(());
                }
            };

            action(self);
        }

        Ok(())
    }

    fn setup_input_events(&self) -> AbortableTask<()> {
        let sender = self.sender().clone();
        let ctrl_c = InputEvent::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
        });
        let mut reader = EventStream::new();

        utils::dispatch_abortable_task(async move {
            loop {
                let event = reader.next();

                if let Some(Ok(mut event)) = event.await {
                    // handle ctrl_c
                    if event == ctrl_c {
                        sender.send_with_urgency(Event::CtrlC);
                    } else if let InputEvent::Resize(x, y) = &mut event {
                        // Resize events can occur in batches.
                        // With a simple loop they can be flushed.
                        // return whether resize event was flushed
                        while let Ok(true) = crossterm::event::poll(Duration::from_millis(50)) {
                            if let Ok(InputEvent::Resize(resize_x, resize_y)) =
                                crossterm::event::read()
                            {
                                *x = resize_x;
                                *y = resize_y;
                            }
                        }
                        sender.send_with_urgency(Event::Input(event))
                    } else {
                        sender.send(Event::Input(event))
                    }
                } else {
                    break;
                }
            }
        })
    }

    /// Start the main loop.
    ///
    /// This will listen to events and do the appropriate actions.
    pub async fn start(&mut self) -> Result<()> {
        let mut stdout = std::io::stdout();

        enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

        let input_task_handle = self.setup_input_events();
        let mut terminal = Terminal::new(ui::backend()).unwrap();
        // TODO: display notifications if any.

        while !self.should_exit {
            terminal.draw(|frame| self.get_current_view().draw(frame, self))?;

            match self.event_queue.recv_async().await.unwrap() {
                // handle network events
                Event::Net(net_event) => self.handle_net_event(net_event)?,

                // handle input events
                Event::Input(event) => self.handle_input_event(event)?,

                // close notification
                Event::CloseNotification => {
                    self.notifications.pop();
                }

                // close on ctrl-c
                Event::CtrlC => self.exit(),
            }
        }

        self.server.disconnect();

        // stop listening for inputs
        input_task_handle.abort();

        execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
        disable_raw_mode()?;
        Ok(())
    }
}
