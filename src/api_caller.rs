use parking_lot::Mutex;
use rayon::iter::{ParallelExtend, ParallelIterator};
use rayon::{
    iter::{IntoParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator},
    ThreadPool, ThreadPoolBuilder,
};
use serde::Deserialize;
use serde::Serialize;
use serde_json::from_str;
use std::{
    collections::{HashMap, VecDeque},
    io::Write,
};

use reqwest::blocking::Client;

const URL: &str = "https://api.steampowered.com/";

#[derive(Debug, Deserialize)]
struct Body {
    friendslist: FriendsList,
}

#[derive(Debug, Deserialize)]
struct FriendsList {
    friends: Vec<Friend>,
}

#[derive(Debug, Deserialize)]
struct Friend {
    steamid: String,
    relationship: String,
    friend_since: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GamesResponse {
    response: Games,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Games {
    game_count: usize,
    games: Vec<Game>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Game {
    pub appid: usize,
    pub playtime_forever: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PlayerGames {
    player: String,
    pub games: Vec<Game>,
}

fn make_api_call(
    client: &mut Client,
    url: &str,
    params: &HashMap<String, String>,
) -> Result<String, String> {
    let url = String::from(URL) + url;
    loop {
        let req = client
            .get(&url)
            .query(params)
            .send()
            .map_err(|e| format!("{}", e))?;
        let xeresult = req
            .headers()
            .get("x-eresult")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        if xeresult != "1" {
            println!(
                "x-eresult: {}",
                req.headers().get("x-eresult").unwrap().to_str().unwrap()
            );
            if xeresult == "84" {
                std::thread::sleep(std::time::Duration::from_secs(10));
            } else {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            continue;
        }
        break req.text().map_err(|e| format!("{}", e));
    }
}

pub(crate) fn crawl(token: &str, seed: &str) -> Result<Mutex<HashMap<String, usize>>, String> {
    let network = Mutex::new(HashMap::<String, usize>::new());
    network.lock().insert(String::from("private"), 0);
    let raw_file = Mutex::new(std::fs::File::create("raw.txt").map_err(|e| format!("{}", e))?);
    let network_queue = Mutex::new(VecDeque::<String>::new());
    let mut params = HashMap::<String, String>::new();
    params.insert(String::from("relationship"), String::from("friend"));
    params.insert(String::from("key"), token.to_string());
    network_queue.lock().push_back(seed.to_string());

    rayon::scope(|s| {
        for _ in 0..8 {
            s.spawn(|_| loop {
                let id = match network_queue.lock().pop_front() {
                    Some(id) => id,
                    None => break,
                };
                println!("{}: {}", rayon::current_thread_index().unwrap(), id);

                network.lock().insert(id.clone(), 1);

                let mut params = params.clone();
                params.insert(String::from("steamid"), id.clone());
                let text = match make_api_call(
                    &mut Client::new(),
                    "ISteamUser/GetFriendList/v0001/",
                    &params,
                ) {
                    Ok(text) => text,
                    Err(e) => {
                        eprintln!("reading API: {}", e);
                        continue;
                    }
                };

                let body: Body = match from_str(&text) {
                    Ok(body) => body,
                    Err(e) => {
                        eprintln!("parsing body '{}': {}", text, e);
                        *network.lock().get_mut("private").unwrap() += 1;
                        continue;
                    }
                };

                for friend in body.friendslist.friends {
                    if let Some(f) = network.lock().get_mut(&friend.steamid) {
                        println!("Hit {} {} times", friend.steamid, f);
                        *f += 1;
                    } else {
                        writeln!(raw_file.lock(), "{}", friend.steamid);
                        network_queue.lock().push_front(friend.steamid);
                    }
                }
            });
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });

    Ok(network)
}

pub(crate) fn collect_game_info(token: &str, user_id: &str) -> Result<PlayerGames, String> {
    let mut params = HashMap::<String, String>::new();
    params.insert(String::from("key"), token.to_string());
    params.insert(String::from("steamid"), user_id.to_string());

    let res = make_api_call(
        &mut Client::new(),
        "IPlayerService/GetOwnedGames/v0001",
        &params,
    )
    .map_err(|e| format!("{}", e))?;
    let games: GamesResponse = from_str(&res).map_err(|e| format!("{}: {}", res, e))?;
    Ok(PlayerGames {
        player: user_id.to_string(),
        games: games.response.games,
    })
}
