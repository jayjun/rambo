use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time;

enum Message {
    Command(String),
    Arg(String),
    Stdin(Vec<u8>),
    Env(String, String),
    CurrentDir(String),
    Error(String),
    ExitStatus(i32),
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    EOT,
}

const COMMAND: u8 = 0;
const ARG: u8 = 1;
const STDIN: u8 = 2;
const ENV: u8 = 3;
const CURRENT_DIR: u8 = 4;
const EOT: u8 = 5;
const ERROR: u8 = 6;
const STDOUT: u8 = 7;
const STDERR: u8 = 8;
const EXIT_STATUS: u8 = 9;

impl Message {
    fn from_bytes(bytes: Vec<u8>) -> Message {
        match bytes[0] {
            COMMAND => Message::Command(std::str::from_utf8(&bytes[1..]).unwrap().to_string()),
            ARG => Message::Arg(std::str::from_utf8(&bytes[1..]).unwrap().to_string()),
            STDIN => Message::Stdin((&bytes[1..]).iter().cloned().collect()),
            ENV => {
                let mut name_length: [u8; 4] = [0, 0, 0, 0];
                name_length.copy_from_slice(&bytes[1..5]);
                let name_length = u32::from_be_bytes(name_length) as usize;
                let name_end = 5 + name_length;
                let name: String = std::str::from_utf8(&bytes[5..name_end])
                    .unwrap()
                    .to_string();
                let value: String = std::str::from_utf8(&bytes[name_end..]).unwrap().to_string();
                Message::Env(name, value)
            }
            CURRENT_DIR => {
                Message::CurrentDir(std::str::from_utf8(&bytes[1..]).unwrap().to_string())
            }
            EOT => Message::EOT,
            message_type => panic!(
                "from_bytes() received unexpected message type {}",
                message_type
            ),
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        match self {
            Message::Error(message) => Message::to_vec(ERROR, message.as_bytes()),
            Message::Stdout(buffer) => Message::to_vec(STDOUT, buffer.as_slice()),
            Message::Stderr(buffer) => Message::to_vec(STDERR, buffer.as_slice()),
            Message::ExitStatus(code) => Message::to_vec(EXIT_STATUS, &code.to_be_bytes()),
            _ => panic!("to_bytes() only implemented for outgoing messages"),
        }
    }

    fn to_vec(message_type: u8, bytes: &[u8]) -> Vec<u8> {
        let length: [u8; 4] = ((1 + bytes.len()) as u32).to_be_bytes();
        let mut buffer: Vec<u8> = length.iter().cloned().collect();
        buffer.push(message_type);
        buffer.extend(bytes);
        buffer
    }

    fn read_from_erlang() -> io::Result<Message> {
        let mut length: [u8; 4] = [0, 0, 0, 0];
        io::stdin().read_exact(&mut length)?;
        let length = u32::from_be_bytes(length) as usize;

        let mut buffer: Vec<u8> = vec![0; length];
        io::stdin()
            .read_exact(&mut buffer)
            .map(|_| Message::from_bytes(buffer))
    }

    // Write to standard output can fail but don't handle it. Because the only
    // sane way to handle this error is to notify Erlang, but we can't. Instead
    // of failing silently, we can try to exit. But if the Erlang node isn't
    // there, it means standard input has closed and we are exiting anyway.
    fn write_to_erlang(&self) {
        let buffer = self.to_bytes();
        if let Ok(()) = io::stdout().write_all(&buffer) {
            let _ = io::stdout().flush();
        };
    }

    fn stream_to_erlang<R, F>(mut buffer_reader: io::BufReader<R>, write_all: F)
    where
        R: io::Read,
        F: Fn(Vec<u8>),
    {
        let mut buffer = vec![];

        while let Ok(bytes_read) = buffer_reader.read_until(b'\n', &mut buffer) {
            if bytes_read == 0 {
                break;
            }
            write_all(buffer.clone());
            buffer.clear();
        }
    }
}

trait Sendable {
    fn send_to_erlang(self);
}

impl Sendable for io::Result<()> {
    fn send_to_erlang(self) {
        if let Err(error) = self {
            Message::Error(format!("{}", error)).write_to_erlang();
        }
    }
}

impl Sendable for io::Result<process::ExitStatus> {
    fn send_to_erlang(self) {
        match self {
            Ok(exit_status) => {
                if let Some(code) = exit_status.code() {
                    Message::ExitStatus(code).write_to_erlang()
                }
            }
            Err(error) => Message::Error(format!("{}", error)).write_to_erlang(),
        };
    }
}

impl Sendable for process::ChildStdout {
    fn send_to_erlang(self) {
        Message::stream_to_erlang(io::BufReader::new(self), |buffer| {
            Message::Stdout(buffer).write_to_erlang();
        });
    }
}

impl Sendable for process::ChildStderr {
    fn send_to_erlang(self) {
        Message::stream_to_erlang(io::BufReader::new(self), |buffer| {
            Message::Stderr(buffer).write_to_erlang();
        });
    }
}

struct Command {
    command: Option<String>,
    args: Vec<String>,
    stdin: Option<Vec<u8>>,
    envs: HashMap<String, String>,
    current_dir: Option<String>,
}

impl Command {
    fn new() -> Command {
        Command {
            command: None,
            args: vec![],
            stdin: None,
            envs: HashMap::new(),
            current_dir: None,
        }
    }

    fn add(&mut self, message: Message) {
        match message {
            Message::Command(command) => self.command = Some(command),
            Message::Arg(arg) => self.args.push(arg),
            Message::Stdin(stdin) => self.stdin = Some(stdin),
            Message::Env(name, value) => {
                self.envs.insert(name, value);
            }
            Message::CurrentDir(current_dir) => self.current_dir = Some(current_dir),
            _ => (),
        }
    }

    fn run(&mut self, kill: Arc<AtomicBool>, pair: Arc<(Mutex<bool>, Condvar)>) -> io::Result<()> {
        let command = self.command.as_ref().expect("Rambo requires a command!");
        let mut command = process::Command::new(command);
        command
            .args(self.args.clone())
            .envs(self.envs.clone())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped());

        if self.stdin.is_some() {
            command.stdin(process::Stdio::piped());
        }

        if let Some(current_dir) = self.current_dir.as_ref() {
            command.current_dir(current_dir);
        }

        let mut child = command.spawn()?;

        if let Some(stdin) = self.stdin.as_ref() {
            let error = io::Error::new(io::ErrorKind::Other, "Child stdin cannot be opened");
            child.stdin.take().ok_or(error)?.write_all(stdin)?;
        }

        drop(child.stdin.take());

        let error = io::Error::new(io::ErrorKind::Other, "Child stdout cannot be read");
        let stdout = child.stdout.take().ok_or(error)?;
        thread::spawn(move || stdout.send_to_erlang());

        let error = io::Error::new(io::ErrorKind::Other, "Child stderr cannot be read");
        let stderr = child.stderr.take().ok_or(error)?;
        thread::spawn(move || stderr.send_to_erlang());

        thread::spawn(move || {
            loop {
                // Busy looping try_wait is not great. Using waitid to wait
                // without reaping is better but Rust libc has not implemented
                // the si_status field in siginfo_t so there is no way to get
                // the child's exit status from waitid.
                match child.try_wait() {
                    Ok(None) => {
                        // Child has not exited. Kill if allowed.
                        if kill.load(Ordering::Relaxed) {
                            let _ = child.kill();
                        }
                        thread::sleep(time::Duration::from_millis(1));
                    }
                    Ok(Some(exit_status)) => {
                        Ok(exit_status).send_to_erlang();
                        break;
                    }
                    Err(error) => {
                        let error: io::Result<process::ExitStatus> = Err(error);
                        error.send_to_erlang();
                        break;
                    }
                }
            }

            let &(ref exited, ref condvar) = &*pair;
            let mut exited = exited.lock().unwrap();
            *exited = true;
            condvar.notify_all();
        });

        Ok(())
    }
}

fn main() {
    let mut command = Command::new();
    let kill = Arc::new(AtomicBool::new(false));
    let pair = Arc::new((Mutex::new(false), Condvar::new()));

    // Returns Err(std::io::ErrorKind::UnexpectedEof) when standard input is
    // closed. This means the port is closed or Erlang node has stopped. So exit
    // immediately to avoid becoming an orphan (i.e. process leak).
    while let Ok(message) = Message::read_from_erlang() {
        match message {
            Message::EOT => {
                command.run(kill.clone(), pair.clone()).send_to_erlang();
            }
            message => command.add(message),
        }
    }

    // Allow kill
    kill.swap(true, Ordering::Relaxed);

    // Wait for child to exit
    let &(ref exited, ref condvar) = &*pair;
    let mut exited = exited.lock().unwrap();
    while !*exited {
        exited = condvar.wait(exited).unwrap();
    }
}
