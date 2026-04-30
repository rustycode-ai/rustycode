use std::path::Path;

fn main() {
    let p = Path::new(".env");
    println!("Extension: {:?}", p.extension());
    println!("File name: {:?}", p.file_name());
    println!("File stem: {:?}", p.file_stem());
}
