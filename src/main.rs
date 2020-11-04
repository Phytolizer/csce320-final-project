use std::{collections::HashMap, fs::File, io::BufRead, io::BufReader};

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

    let file = Mutex::new(BufReader::new(File::open("raw.txt").unwrap()));
    let data = Mutex::new(Vec::<PlayerGames>::new());
    let token = String::from_utf8_lossy(&std::fs::read("token.txt").unwrap())
        .trim()
        .to_string();
    rayon::scope(|s| {
        for _ in 0..8 {
            s.spawn(|_| {
                let mut line = String::new();
                loop {
                    if let Ok(0) = file.lock().read_line(&mut line) {
                        break;
                    }
                    let games = match api_caller::collect_game_info(&token, line.trim()) {
                        Ok(games) => games,
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
                    data.lock().push(games);
                    line.clear();
                }
            });
        }
    });
    std::fs::write(
        "games.json",
        to_string_pretty(&data.lock() as &[_]).unwrap(),
    )
    .unwrap();
}
