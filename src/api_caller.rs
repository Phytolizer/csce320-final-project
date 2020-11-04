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

// these structs allow to parse JSON into meaningful data with named fields
// that's what #[derive(Deserialize)] means

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

// pub(crate) === visible outside this file
// without putting pub(crate) on something, you can't use it in another file
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
    // pub rule applies to fields too
    pub appid: usize,
    pub playtime_forever: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PlayerGames {
    player: String,
    pub games: Vec<Game>,
}

/// Make an API call, repeating it until it succeeds.
fn make_api_call(
    client: &mut Client,
    url: &str,
    params: &HashMap<String, String>,
) -> Result<String, String> {
    let url = String::from(URL) + url;
    // while (true)
    loop {
        // send the request
        let req = client
            .get(&url)
            .query(params)
            .send()
            .map_err(|e| format!("{}", e))?;
        // check for errors in the response headers
        let xeresult = req
            .headers()
            .get("x-eresult")
            // header will always have the key 'x-eresult'
            .unwrap()
            .to_str()
            // the x-eresult will always be valid to put in a string
            .unwrap()
            .to_string();
        // "1" means "OK"
        if xeresult != "1" {
            println!(
                "x-eresult: {}",
                req.headers().get("x-eresult").unwrap().to_str().unwrap()
            );
            if xeresult == "84" {
                // "84" means "rate limited"
                std::thread::sleep(std::time::Duration::from_secs(10));
            } else {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            continue;
        }
        // map_err here converts the error to a string
        return req.text().map_err(|e| format!("{}", e));
    }
}

// this function can be called from other files

/// Crawl a user's friend list recursively.
/// This can take a VERY long time.
///
/// Returns: a map with keys representing users and values representing the number
/// of hits that user received from the crawler
pub(crate) fn crawl(token: &str, seed: &str) -> Result<Mutex<HashMap<String, usize>>, String> {
    // thread-safe map holding the "network" of friends
    let network = Mutex::new(HashMap::<String, usize>::new());
    // the key "private" represents all private profiles
    network.lock().insert(String::from("private"), 0);
    // raw_file will be updated continuously with the unique user IDs only
    // this is a backup in case the program somehow crashes
    let raw_file = Mutex::new(std::fs::File::create("raw.txt").map_err(|e| format!("{}", e))?);
    // the queue holds the user IDs yet to be crawled
    let network_queue = Mutex::new(VecDeque::<String>::new());
    let mut params = HashMap::<String, String>::new();
    // required key for HTTP query, 'relationship=friend'
    params.insert(String::from("relationship"), String::from("friend"));
    // required for this API call, the token to authenticate the crawler with
    params.insert(String::from("key"), token.to_string());
    // start with the "seed" user ID
    network_queue.lock().push_back(seed.to_string());

    rayon::scope(|s| {
        // spawn 8 threads
        for _ in 0..8 {
            // infinite loop
            s.spawn(|_| loop {
                // if the queue is empty, stop
                let id = match network_queue.lock().pop_front() {
                    // pop_front returns something called Option, with two possible values: Some and None
                    // if it's Some, it carries with it the user ID
                    Some(id) => id,
                    // if it's None, the queue was empty
                    None => break,
                };
                // logging
                println!("{}: {}", rayon::current_thread_index().unwrap(), id);

                // this is a unique ID
                network.lock().insert(id.clone(), 1);

                let mut params = params.clone();
                // add one last param to the query: steamid=<id>
                params.insert(String::from("steamid"), id.clone());
                let text = match make_api_call(
                    &mut Client::new(),
                    "ISteamUser/GetFriendList/v0001/",
                    &params,
                ) {
                    // API call succeeded :)
                    Ok(text) => text,
                    // API call failed :(
                    Err(e) => {
                        eprintln!("reading API: {}", e);
                        // ignore the error and continue harassing Steam
                        continue;
                    }
                };

                // parse the JSON response into the Body struct
                // it's **magic**
                let body: Body = match from_str(&text) {
                    Ok(body) => body,
                    Err(e) => {
                        // private user
                        eprintln!("parsing body '{}': {}", text, e);
                        *network.lock().get_mut("private").unwrap() += 1;
                        continue;
                    }
                };

                // now add all their friends to the queue
                for friend in body.friendslist.friends {
                    if let Some(f) = network.lock().get_mut(&friend.steamid) {
                        // repeat user
                        println!("Hit {} {} times", friend.steamid, f);
                        *f += 1;
                    } else {
                        // unique user
                        writeln!(raw_file.lock(), "{}", friend.steamid);
                        network_queue.lock().push_front(friend.steamid);
                    }
                }
            });
            // this sleep allows the friends of the seed to be added to the queue
            // otherwise, only 1 thread would end up running at all
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
    // parse as GamesResponse, again **magic**
    let games: GamesResponse = from_str(&res).map_err(|e| format!("{}: {}", res, e))?;
    Ok(PlayerGames {
        player: user_id.to_string(),
        games: games.response.games,
    })
}
