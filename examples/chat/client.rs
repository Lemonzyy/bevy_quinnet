use std::{
    collections::HashMap,
    thread::{self, sleep},
    time::Duration,
};

use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    log::LogPlugin,
    prelude::{info, warn, App, Commands, CoreStage, EventReader, EventWriter, Res, ResMut},
};
use bevy_quinnet::{
    client::{
        certificate::CertificateVerificationMode, Client, ClientConfigurationData, ConnectionEvent,
        QuinnetClientPlugin,
    },
    ClientId,
};
use rand::{distributions::Alphanumeric, Rng};
use tokio::sync::mpsc;

use protocol::{ClientMessage, ServerMessage};

mod protocol;

#[derive(Debug, Clone, Default)]
struct Users {
    self_id: ClientId,
    names: HashMap<ClientId, String>,
}

pub fn on_app_exit(app_exit_events: EventReader<AppExit>, client: Res<Client>) {
    if !app_exit_events.is_empty() {
        client.send_message(ClientMessage::Disconnect {}).unwrap();
        // TODO Clean: event to let the async client send his last messages.
        sleep(Duration::from_secs_f32(0.1));
    }
}

fn handle_server_messages(mut client: ResMut<Client>, mut users: ResMut<Users>) {
    while let Ok(Some(message)) = client.receive_message::<ServerMessage>() {
        match message {
            ServerMessage::ClientConnected {
                client_id,
                username,
            } => {
                info!("{} joined", username);
                users.names.insert(client_id, username);
            }
            ServerMessage::ClientDisconnected { client_id } => {
                if let Some(username) = users.names.remove(&client_id) {
                    println!("{} left", username);
                } else {
                    warn!("ClientDisconnected for an unknown client_id: {}", client_id)
                }
            }
            ServerMessage::ChatMessage { client_id, message } => {
                if let Some(username) = users.names.get(&client_id) {
                    if client_id != users.self_id {
                        println!("{}: {}", username, message);
                    }
                } else {
                    warn!("Chat message from an unknown client_id: {}", client_id)
                }
            }
            ServerMessage::InitClient {
                client_id,
                usernames,
            } => {
                users.self_id = client_id;
                users.names = usernames;
            }
        }
    }
}

fn handle_terminal_messages(
    client: ResMut<Client>,
    mut terminal_messages: ResMut<mpsc::Receiver<String>>,
    mut app_exit_events: EventWriter<AppExit>,
) {
    while let Ok(message) = terminal_messages.try_recv() {
        if message == "quit" {
            app_exit_events.send(AppExit);
        } else {
            client
                .send_message(ClientMessage::ChatMessage { message: message })
                .expect("Failed to send chat message");
        }
    }
}

fn start_terminal_listener(mut commands: Commands) {
    let (from_terminal_sender, from_terminal_receiver) = mpsc::channel::<String>(100);

    thread::spawn(move || loop {
        let mut buffer = String::new();
        std::io::stdin().read_line(&mut buffer).unwrap();
        from_terminal_sender
            .try_send(buffer.trim_end().to_string())
            .unwrap();
    });

    commands.insert_resource(from_terminal_receiver);
}

fn start_connection(client: ResMut<Client>) {
    client
        .connect(
            ClientConfigurationData::new("127.0.0.1".to_string(), 6000, "0.0.0.0".to_string(), 0),
            CertificateVerificationMode::SkipVerification,
        )
        .unwrap();

    // You can already send message(s) even before being connected, they will be buffered. In this example we will wait for a ConnectionEvent. We could also check client.is_connected()
}

fn handle_client_events(connection_events: EventReader<ConnectionEvent>, client: ResMut<Client>) {
    if !connection_events.is_empty() {
        // We are connected
        let username: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(7)
            .map(char::from)
            .collect();

        println!("--- Joining with name: {}", username);
        println!("--- Type 'quit' to disconnect");

        client
            .send_message(ClientMessage::Join { name: username })
            .unwrap();

        connection_events.clear();
    }
}

fn main() {
    App::new()
        .add_plugin(ScheduleRunnerPlugin::default())
        .add_plugin(LogPlugin::default())
        .add_plugin(QuinnetClientPlugin::default())
        .insert_resource(Users::default())
        .add_startup_system(start_terminal_listener)
        .add_startup_system(start_connection)
        .add_system(handle_terminal_messages)
        .add_system(handle_server_messages)
        .add_system(handle_client_events)
        // CoreStage::PostUpdate so that AppExit events generated in the previous stage are available
        .add_system_to_stage(CoreStage::PostUpdate, on_app_exit)
        .run();
}
