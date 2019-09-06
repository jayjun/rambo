use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::process;
use std::thread;

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
const EXIT_STATUS: u8 = 7;
const STDOUT: u8 = 8;
const STDERR: u8 = 9;

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
            Message::ExitStatus(code) => Message::to_vec(EXIT_STATUS, &code.to_be_bytes()),
            Message::Stdout(buffer) => Message::to_vec(STDOUT, buffer.as_slice()),
            Message::Stderr(buffer) => Message::to_vec(STDERR, buffer.as_slice()),
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

    fn write_to_erlang(&self) -> io::Result<()> {
        let buffer = self.to_bytes();
        io::stdout().write_all(&buffer)?;
        io::stdout().flush()
    }

    fn stream_to_erlang<R, F>(mut buffer_reader: io::BufReader<R>, write_all: F) -> io::Result<()>
    where
        R: io::Read,
        F: Fn(Vec<u8>) -> io::Result<()>,
    {
        let mut buffer = vec![];

        while let Ok(bytes_read) = buffer_reader.read_until(b'\n', &mut buffer) {
            if bytes_read == 0 {
                break;
            }
            write_all(buffer.clone())?;
            buffer.clear();
        }

        Ok(())
    }
}

trait Sendable {
    fn send_to_erlang(self) -> io::Result<()>;
}

impl Sendable for io::Result<process::ExitStatus> {
    fn send_to_erlang(self) -> io::Result<()> {
        let message = match self {
            Ok(exit_status) => {
                let exit_status = exit_status.code().expect("Child should have exit status");
                Message::ExitStatus(exit_status)
            }
            Err(error) => Message::Error(format!("{}", error)),
        };
        message.write_to_erlang()
    }
}

impl Sendable for process::ChildStdout {
    fn send_to_erlang(self) -> io::Result<()> {
        Message::stream_to_erlang(io::BufReader::new(self), |buffer| {
            Message::Stdout(buffer).write_to_erlang()
        })
    }
}

impl Sendable for process::ChildStderr {
    fn send_to_erlang(self) -> io::Result<()> {
        Message::stream_to_erlang(io::BufReader::new(self), |buffer| {
            Message::Stderr(buffer).write_to_erlang()
        })
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

    fn run(&self) -> io::Result<process::ExitStatus> {
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

        let mut handles = Vec::new();

        let error = io::Error::new(io::ErrorKind::Other, "Child stdout cannot be read");
        let stdout = child.stdout.take().ok_or(error)?;
        handles.push(thread::spawn(move || stdout.send_to_erlang()));

        let error = io::Error::new(io::ErrorKind::Other, "Child stderr cannot be read");
        let stderr = child.stderr.take().ok_or(error)?;
        handles.push(thread::spawn(move || stderr.send_to_erlang()));

        child.wait()
    }
}

fn main() {
    let mut command = Command::new();

    // Returns Err(std::io::ErrorKind::UnexpectedEof) if stdin is closed, which
    // means the port is closed or Erlang node is dead. So exit immediately to
    // avoid becoming an orphan (i.e. process leak).
    while let Ok(message) = Message::read_from_erlang() {
        match message {
            Message::EOT => {
                let _ = command.run().send_to_erlang();
            }
            message => command.add(message),
        }
    }
}
