// memod: daemon process. Owns all filesystem I/O.
// Serves an axum/tokio HTTP API on 127.0.0.1:18301.

pub mod db;

fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn main_executes() {
        super::main();
    }
}
