use std::{
    collections::HashMap,
    fs::File,
    io::BufRead,
    io::{BufReader, Write},
};

use api_caller::{crawl, PlayerGames};
use parking_lot::Mutex;
use serde_json::to_string_pretty;

const STEAM_API_URL: &str = "https://api.steampowered.com/";

mod api_caller;
mod state;

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

    // BufReader allows reading a file line by line
    let file = Mutex::new(BufReader::new(File::open("raw.txt").unwrap()));
    // C++ equivalent: std::pair<std::mutex, std::vector<PlayerGames>>
    // can't access data without locking it first
    let data = Mutex::new(Vec::<PlayerGames>::new());
    let games_raw = Mutex::new(File::create("games_raw.txt").unwrap());
    // holds the contents of token.txt
    let token = String::from_utf8_lossy(&std::fs::read("token.txt").unwrap())
        .trim()
        .to_string();
    rayon::scope(|s| {
        // spawn 8 threads
        for _ in 0..8 {
            s.spawn(|_| {
                // buffer for the current line
                let mut line = String::new();
                loop {
                    // read a line and check for EOF
                    if let Ok(0) = file.lock().read_line(&mut line) {
                        break;
                    }
                    // this 'match' statement checks for errors, similar to try/catch
                    let games = match api_caller::collect_game_info(
                        &token,
                        line.trim(),
                    ) {
                        // function was successful
                        Ok(games) => games,
                        // there was some error, print it but otherwise ignore it
                        Err(e) => {
                            eprintln!("{}", e);
                            line.clear();
                            continue;
                        }
                    };
                    println!(
                        "{}: {} owns {} games",
                        rayon::current_thread_index().unwrap(),
                        line.trim(),
                        games.games.len()
                    );
                    // append to data
                    games_raw.lock().write_all(
                        to_string_pretty(&games).unwrap().as_bytes(),
                    );
                    data.lock().push(games);
                    line.clear();
                }
            });
        }
    });
    // convert to JSON and save to file
    std::fs::write(
        "games.json",
        // magic
        to_string_pretty(&data.lock() as &[_]).unwrap(),
    )
    .unwrap();
}
