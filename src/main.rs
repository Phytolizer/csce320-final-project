use itertools::Itertools;
use std::{
    collections::HashMap,
    fs,
    fs::File,
    io::BufRead,
    io::{BufReader, Write},
};

use api_caller::{crawl, GameAndReviewInfo, PlayerGames};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;

const STEAM_API_URL: &str = "https://store.steampowered.com";

mod api_caller;
mod state;

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Hash)]
struct RawGames {
    games: Vec<RawGame>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Hash)]
struct RawGame {
    appid: u128,
    playtime_forever: u128,
}

fn main() {
    // let data = match crawl(
    //     String::from_utf8_lossy(&std::fs::read("token.txt").unwrap()).trim(),
    //     "76561198021266721",
    // ) {
    //     Ok(data) => data,
    //     Err(e) => {
    //         eprintln!("{}", e);
    //         return;
    //     }
    // };
    // std::fs::write(
    //     "out.json",
    //     to_string_pretty(&data.lock() as &HashMap<String, usize>).unwrap(),
    // );

    // // BufReader allows reading a file line by line
    // let file = Mutex::new(BufReader::new(File::open("raw.txt").unwrap()));
    // // C++ equivalent: std::pair<std::mutex, std::vector<PlayerGames>>
    // // can't access data without locking it first
    // let data = Mutex::new(Vec::<PlayerGames>::new());
    // let games_raw = Mutex::new(File::create("games_raw.txt").unwrap());
    // // holds the contents of token.txt
    // let token = String::from_utf8_lossy(&std::fs::read("token.txt").unwrap())
    //     .trim()
    //     .to_string();
    // rayon::scope(|s| {
    //     // spawn 8 threads
    //     for _ in 0..8 {
    //         s.spawn(|_| {
    //             // buffer for the current line
    //             let mut line = String::new();
    //             loop {
    //                 // read a line and check for EOF
    //                 if let Ok(0) = file.lock().read_line(&mut line) {
    //                     break;
    //                 }
    //                 // this 'match' statement checks for errors, similar to try/catch
    //                 let games = match api_caller::collect_game_info(
    //                     &token,
    //                     line.trim(),
    //                 ) {
    //                     // function was successful
    //                     Ok(games) => games,
    //                     // there was some error, print it but otherwise ignore it
    //                     Err(e) => {
    //                         eprintln!("{}", e);
    //                         line.clear();
    //                         continue;
    //                     }
    //                 };
    //                 println!(
    //                     "{}: {} owns {} games",
    //                     rayon::current_thread_index().unwrap(),
    //                     line.trim(),
    //                     games.games.len()
    //                 );
    //                 // append to data
    //                 games_raw.lock().write_all(
    //                     to_string_pretty(&games).unwrap().as_bytes(),
    //                 );
    //                 data.lock().push(games);
    //                 line.clear();
    //             }
    //         });
    //     }
    // });
    // // convert to JSON and save to file
    // std::fs::write(
    //     "games.json",
    //     // magic
    //     to_string_pretty(&data.lock() as &[_]).unwrap(),
    // )
    // .unwrap();

    let game_info_raw = Mutex::new(File::create("game_info_raw.txt").unwrap());
    let file_contents =
        String::from_utf8(fs::read("games_raw.txt").unwrap()).unwrap();
    let split_contents = Mutex::new(
        file_contents
            .split("}{")
            .filter(|s| !s.is_empty())
            .map(|o| format!("{{{}}}", o))
            .map(|o| serde_json::from_str::<RawGames>(&o).unwrap())
            .flat_map(|rgs| rgs.games)
            .unique_by(|rg| rg.appid),
    );
    #[allow(clippy::while_let_loop)]
    rayon::scope(|s| {
        for _ in 0..8 {
            s.spawn(|_| loop {
                let raw_games = match split_contents.lock().next() {
                    Some(rg) => rg,
                    None => break,
                };
                let game_and_review_info =
                    match api_caller::get_info_for_game(raw_games.appid) {
                        Ok(gri) => gri,
                        Err(e) => {
                            println!("{}", e);
                            continue;
                        }
                    };
                write!(
                    game_info_raw.lock(),
                    "{}",
                    to_string_pretty(&game_and_review_info).unwrap()
                )
                .unwrap();
            })
        }
    });
}
