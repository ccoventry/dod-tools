use analysis::Analysis;
use std::fs;

fn main() {
    let path = "demos/bewton-playoffs-round1-armory-allied.dem";
    println!("=== Parsing Allied Demo ===");
    let file_bytes = fs::read(path).unwrap();
    let _analysis = Analysis::try_from_bytes(&file_bytes).unwrap();
    println!("=== Parsing Finished ===");
}
