use parking_lot::Mutex;
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

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ReviewResult {
    success: i32,
    query_summary: ReviewSummary,
    reviews: Vec<Review>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ReviewSummary {
    review_score: f64,
    total_positive: usize,
    total_negative: usize,
    total_reviews: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Review {
    weighted_vote_score: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) enum StringOrUsize {
    Usize(usize),
    String(String),
}

impl From<usize> for StringOrUsize {
    fn from(u: usize) -> Self {
        Self::Usize(u)
    }
}

impl From<String> for StringOrUsize {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct GameInfoResult {
    success: bool,
    data: Option<GameInfo>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct GameInfo {
    name: String,
    steam_appid: u128,
    genres: Option<Vec<Genre>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Genre {
    description: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct GameAndReviewInfo {
    game_info: GameInfo,
    reviews: Vec<Review>,
    review_summary: ReviewSummary,
}

/// Make an API call, repeating it until it succeeds.
fn make_api_call(
    client: &mut Client,
    url: &str,
    params: &HashMap<String, String>,
) -> Result<String, String> {
    let url = String::from(crate::STEAM_API_URL) + url;
    // while (true)
    // send the request
    let response = loop {
        let response = client
            .get(&url)
            .query(params)
            .send()
            .map_err(|e| format!("{}", e))?;
        match response.status() {
            reqwest::StatusCode::OK => break response,
            reqwest::StatusCode::TOO_MANY_REQUESTS
            | reqwest::StatusCode::FORBIDDEN => {
                println!(
                    "thread {}: FAIL: {} -- waiting 60 seconds",
                    rayon::current_thread_index().unwrap(),
                    response.status()
                );
                std::thread::sleep(std::time::Duration::from_secs(60));
                continue;
            }
            _ => {
                println!(
                    "thread {}: CRITICAL -- received unknown status!",
                    rayon::current_thread_index().unwrap()
                );
                dbg!(response);
                continue;
            }
        }
    };
    // check for errors in the response headers
    // map_err here converts the error to a string
    response.text().map_err(|e| format!("{}", e))
}

// this function can be called from other files

/// Crawl a user's friend list recursively.
/// This can take a VERY long time.
///
/// Returns: a map with keys representing users and values representing the number
/// of hits that user received from the crawler
pub(crate) fn crawl(
    token: &str,
    seed: &str,
) -> Result<Mutex<HashMap<String, usize>>, String> {
    // thread-safe map holding the "network" of friends
    let network = Mutex::new(HashMap::<String, usize>::new());
    // the key "private" represents all private profiles
    network.lock().insert(String::from("private"), 0);
    // raw_file will be updated continuously with the unique user IDs only
    // this is a backup in case the program somehow crashes
    let raw_file = Mutex::new(
        std::fs::File::create("raw.txt").map_err(|e| format!("{}", e))?,
    );
    // the queue holds the user IDs yet to be crawled
    let network_queue = Mutex::new(VecDeque::<String>::new());
    let mut params = HashMap::<String, String>::new();
    // required key for HTTP query, 'relationship=friend'
    params.insert(String::from("relationship"), String::from("friend"));
    // required for this API call, the token to authenticate the crawler with
    params.insert(String::from("key"), token.to_string());
    // start with the "seed" user ID
    network_queue.lock().push_back(seed.to_string());

    // explanation: using while-let here keeps the lock on network_queue,
    // causing only one thread to execute at a time. this defeats the
    // purpose of parallelizing this code, so it is done with a guarding
    // match statement instead of while let.
    #[allow(clippy::while_let_loop)]
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

pub(crate) fn collect_game_info(
    token: &str,
    user_id: &str,
) -> Result<PlayerGames, String> {
    let mut params = HashMap::<String, String>::new();
    params.insert(String::from("key"), token.to_string());
    params.insert(String::from("steamid"), user_id.to_string());

    let res = make_api_call(
        &mut Client::new(),
        "IPlayerService/GetOwnedGames/v0001",
        &params,
    )
    .map_err(|e| e.to_string())?;
    // parse as GamesResponse, again **magic**
    let games: GamesResponse =
        from_str(&res).map_err(|e| format!("{}: {}", res, e))?;
    Ok(PlayerGames {
        player: user_id.to_string(),
        games: games.response.games,
    })
}

pub(crate) fn get_info_for_game(
    appid: u128,
) -> Result<GameAndReviewInfo, String> {
    // store.steampowered.com/api/appdetails?appid=ABCDEF
    let mut query = HashMap::new();
    query.insert(String::from("appids"), appid.to_string());
    // obtain game info
    let game_data =
        make_api_call(&mut Client::new(), "/api/appdetails", &query)?;
    // parse (this had better work, it's magic though so idk)
    let game_data: serde_json::Value =
        from_str(&game_data).map_err(|e| e.to_string())?;
    let game_info: GameInfoResult = serde_json::from_value(
        game_data
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .clone(),
    )
    .map_err(|e| format!("{}", e))?;
    // store.steampowered.com/appreviews/ABCDEF?json=1
    let mut query = HashMap::new();
    query.insert(String::from("json"), String::from("1"));
    // get review data
    let reviews_text = make_api_call(
        &mut Client::builder().user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:82.0) Gecko/20100101 Firefox/82.0").build().unwrap(),
        &format!("/appreviews/{}", appid),
        &query,
    )?;
    // parse (again, please work :praying_hands:)
    let reviews: ReviewResult = from_str(&reviews_text)
        .map_err(|e| format!("with data {}: {}", reviews_text, e))?;
    if !game_info.success {
        // oh no
        return Err(String::from(
            "Steam indicated a failure retrieving game info",
        ));
    }
    if reviews.success == 0 {
        // uh oh
        return Err(String::from(
            "Steam indicated a failure retrieving review info",
        ));
    }
    // everything parsed correctly!!!!!!
    Ok(GameAndReviewInfo {
        game_info: game_info.data.unwrap(),
        reviews: reviews.reviews,
        review_summary: reviews.query_summary,
    })
}
