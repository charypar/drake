use std::env;

use drake::Drake;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let path = &args[1];

    let mut drake = Drake::new();

    println!("Prinitng swift files in path {}", path);
    drake.print(path)
}
