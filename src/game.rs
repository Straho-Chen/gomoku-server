use std::collections::BTreeMap;

use futures_util::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex},
};
use tokio_tungstenite::WebSocketStream;

pub struct GameManager {
    pub games: BTreeMap<usize, Game>,
    pub waiting_player: Option<Player>,
    pub cnt: usize,
}

impl GameManager {
    pub fn new() -> Self {
        Self {
            games: BTreeMap::new(),
            waiting_player: None,
            cnt: 0,
        }
    }

    pub fn alloc_game_id(&mut self) -> usize {
        self.cnt += 1;
        self.cnt - 1
    }
}

static GAME_MANAGER: Lazy<Mutex<GameManager>> = Lazy::new(|| Mutex::new(GameManager::new()));

pub struct Game {
    game_id: usize,
    player1: Option<Player>,
    player2: Option<Player>,
}

impl Game {
    pub fn new(player1: Player, player2: Player, game_id: usize) -> Self {
        Self {
            player1: Some(player1),
            player2: Some(player2),
            game_id,
        }
    }

    pub fn init(&mut self) {
        let (tx1, rx1) = mpsc::channel(100);
        let (tx2, rx2) = mpsc::channel(100);
        self.player1.as_mut().unwrap().join_game(tx1, rx2);
        self.player2.as_mut().unwrap().join_game(tx2, rx1);
    }

    pub async fn run(&mut self) {
        let mut player1 = self.player1.take().unwrap();
        tokio::spawn(async move {
            player1.run().await;
        });
        let mut player2 = self.player2.take().unwrap();
        tokio::spawn(async move {
            player2.run().await;
        });
    }
}

pub struct Player {
    id: i32,
    pub name: Option<String>,
    pub ws: WebSocketStream<TcpStream>,
    send_ch: Option<mpsc::Sender<Msg>>,
    recv_ch: Option<mpsc::Receiver<Msg>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Msg {
    pub msgtype: String,
    pub squareid: String,
    pub color: String,
    pub message: String,
    pub pair_name: String,
}

impl Player {
    pub fn new(ws: WebSocketStream<TcpStream>) -> Self {
        Self {
            id: 0,
            name: None,
            ws,
            send_ch: None,
            recv_ch: None,
        }
    }

    pub fn join_game(&mut self, send_ch: mpsc::Sender<Msg>, recv_ch: mpsc::Receiver<Msg>) {
        self.send_ch = Some(send_ch);
        self.recv_ch = Some(recv_ch);
    }

    pub async fn run(&mut self) {
        let color = match self.id {
            1 => "white".to_string(),
            2 => "black".to_string(),
            _ => "black".to_string(),
        };
        let pair = Msg {
            msgtype: "pair".to_string(),
            squareid: "0".to_string(),
            color,
            message: "".to_string(),
            pair_name: self.name.clone().unwrap(),
        };

        self.send_ch.as_mut().unwrap().send(pair).await.unwrap();

        let recv_msg = self.recv_ch.as_mut().unwrap().recv().await.unwrap();
        let recv_msg_json = serde_json::to_string(&recv_msg).unwrap();
        println!("{} send message in game: {:?}", self.id, &recv_msg_json);
        self.ws.send(recv_msg_json.into()).await.unwrap();

        if self.id == 2 {
            println!("{} recv", self.id);
            let recv_msg = self.recv_ch.as_mut().unwrap().recv().await.unwrap();
            println!("{} get message:{}", self.id, recv_msg.squareid);
            let recv_msg_json = serde_json::to_string(&recv_msg).expect("failed to convert");
            println!("{} send message in game: {:?}", self.id, &recv_msg_json);
            self.ws.send(recv_msg_json.into()).await.unwrap();
        }
        while let Some(msg) = self.ws.next().await {
            let msg = msg.unwrap();
            if msg.is_text() {
                println!("{} receive message in game: {:?}", self.id, &msg);

                let msg_struct: Msg = serde_json::from_str(msg.to_string().as_str()).unwrap();
                let msg_type = msg_struct.msgtype.as_str();
                match msg_type {
                    "move" => {
                        self.send_ch
                            .as_mut()
                            .unwrap()
                            .send(msg_struct)
                            .await
                            .unwrap();
                        println!("{} send", self.id);
                    }
                    _ => {}
                }

                println!("{} recv", self.id);
                let recv_msg = self.recv_ch.as_mut().unwrap().recv().await.unwrap();
                println!("{} get message:{}", self.id, recv_msg.squareid);
                let recv_msg_json = serde_json::to_string(&recv_msg).expect("failed to convert");
                println!("{} send message in game: {:?}", self.id, &recv_msg_json);
                self.ws.send(recv_msg_json.into()).await.unwrap();
            }
        }
    }
}

pub async fn check_waiting_player(mut new_player: Player) -> bool {
    let mut game_manager = GAME_MANAGER.lock().await;
    if game_manager.waiting_player.is_some() {
        let mut old_player = game_manager.waiting_player.take().unwrap();
        old_player.id = 1;
        new_player.id = 2;
        let game_id = game_manager.alloc_game_id();
        let mut new_game = Game::new(old_player, new_player, game_id);
        new_game.init();

        game_manager.games.insert(game_id, new_game);
        let game = game_manager.games.get_mut(&game_id).unwrap();
        println!("start game! game id {}", game.game_id);
        game.run().await;
        true
    } else {
        game_manager.waiting_player = Some(new_player);
        println!("waiting for another player");
        false
    }
}
