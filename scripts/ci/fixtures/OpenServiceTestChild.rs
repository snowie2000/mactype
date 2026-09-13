use std::{
    env,
    fs,
    io::{self, Write},
    process::{self, Command, Stdio},
    thread,
    time::Duration,
};

fn main() {
    let command = env::args().nth(1).unwrap_or_default();
    let (verb, path) = command.split_once('|').unwrap_or((&command, ""));
    match verb {
        "ok" => {
            print!("stdout-ok");
            eprint!("stderr-ok");
        }
        "fail" => {
            print!("stdout-fail");
            eprint!("stderr-fail");
            process::exit(9);
        }
        "arguments" => {
            let arguments = env::args().skip(2).collect::<Vec<_>>();
            print!("{}", arguments.join("\n"));
        }
        "both-near-limit" => {
            io::stderr().write_all(&vec![b'e'; 60 * 1024]).unwrap();
            io::stderr().flush().unwrap();
            io::stdout().write_all(&vec![b'o'; 60 * 1024]).unwrap();
            io::stdout().flush().unwrap();
        }
        "stdout-overflow" => {
            fs::write(path, process::id().to_string()).unwrap();
            io::stdout().write_all(&vec![b'o'; 70 * 1024]).unwrap();
            io::stdout().flush().unwrap();
            thread::sleep(Duration::from_secs(30));
        }
        "stderr-overflow" => {
            fs::write(path, process::id().to_string()).unwrap();
            io::stderr().write_all(&vec![b'e'; 70 * 1024]).unwrap();
            io::stderr().flush().unwrap();
            thread::sleep(Duration::from_secs(30));
        }
        "timeout" => {
            let mut child = Command::new(env::current_exe().unwrap())
                .arg(format!("child|{path}"))
                .spawn()
                .unwrap();
            thread::sleep(Duration::from_secs(30));
            let _ = child.kill();
        }
        "stdin-stall" => {
            fs::write(path, process::id().to_string()).unwrap();
            thread::sleep(Duration::from_secs(8));
        }
        "exit-with-pipe-descendant" => {
            let assignment_receipt = env::var("MACTYPE_CI_PROCESS_GROUP_READY_FILE").unwrap();
            for _ in 0..500 {
                if fs::metadata(&assignment_receipt).is_ok() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            if fs::metadata(&assignment_receipt).is_err() {
                process::exit(71);
            }
            let child_path = format!("pipe-holder|{path}");
            let child = Command::new(env::current_exe().unwrap())
                .arg(child_path)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            drop(child);
            for _ in 0..200 {
                if fs::metadata(path).is_ok() {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
            process::exit(70);
        }
        "pipe-holder" => {
            fs::write(path, process::id().to_string()).unwrap();
            thread::sleep(Duration::from_secs(8));
        }
        "child" => {
            fs::write(path, process::id().to_string()).unwrap();
            thread::sleep(Duration::from_secs(30));
        }
        _ => process::exit(64),
    }
}
