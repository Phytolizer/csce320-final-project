pub struct State {
    token: String,
}

impl State {
    pub fn new(token: String) -> Self {
        Self {
            token,
            ..Default::default()
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            token: String::new(),
        }
    }
}
