use std::io::Error;

use futures_util::StreamExt;
// use futures_util::{future, StreamExt, TryStreamExt, SinkExt};
use game::{check_waiting_player, Player};
use tokio::net::{TcpListener, TcpStream};

use crate::game::Msg;

mod game;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let addr = "0.0.0.0:9990".to_string();

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(stream));
    }

    Ok(())
}

async fn accept_connection(stream: TcpStream) {
    let addr = stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    println!("Peer address: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    println!("New WebSocket connection: {}", addr);

    let mut player = Player::new(ws_stream);

    while let Some(msg) = player.ws.next().await {
        let msg = msg.unwrap();
        if msg.is_text() {
            println!("Received a message in main: {}", msg);
            let msg_struct: Msg =
                serde_json::from_str(msg.to_text().expect("change to text error in main")).unwrap();
            let msg_type = msg_struct.msgtype.as_str();
            match msg_type {
                "pair" => {
                    player.name = Some(msg_struct.pair_name);
                    break;
                }
                _ => {}
            }
        }
    }

    check_waiting_player(player).await;
}
