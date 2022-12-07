mod library;
use crate::library::io::readers::read_input_from_file;

fn main() {
    // list files from directory
    let dir_name = "data/outputs/";
    let dir = std::fs::read_dir(dir_name).unwrap();
    let mut files: Vec<String> = Vec::new();

    for entry in dir {
        let entry = entry.unwrap();
        let path = entry.path();
        let file_name = path.file_name().unwrap().to_str();
        println!("{:?}", file_name);    
    }

}