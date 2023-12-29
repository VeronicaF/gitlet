use std::path::PathBuf;

fn main() {
    let path = PathBuf::from(".git/objects");

    let dict = path.read_dir().unwrap();

    // iter over the dict and open files
    for entry in dict {
        let entry = entry.unwrap();
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        for d in path.read_dir().unwrap() {
            let d = d.unwrap();
            let path = d.path();
            // call git cat-file -t <sha>
            let sha = path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
                + path.file_name().unwrap().to_str().unwrap();

            println!("{:?}", sha);
            let handler = std::process::Command::new("git")
                .arg("cat-file")
                .arg("-t")
                .arg(sha)
                .spawn()
                .expect("failed to execute process");
            handler.wait_with_output().expect("failed to wait on child");
        }
    }
}
