use futures::future::FutureExt;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::process::{ExitStatus, Stdio};
use tokio::prelude::*;
use tokio::process::{ChildStdin, Command};

#[derive(Debug)]
enum Message {
    Command(String),
    Arg(String),
    Stdin(Vec<u8>),
    Env(String, String),
    CurrentDir(String),
    Eot,
    Error(String),
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    ExitStatus(i32),
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
            COMMAND => Message::Command(Message::string_from_bytes(&bytes[1..])),
            ARG => Message::Arg(Message::string_from_bytes(&bytes[1..])),
            STDIN => Message::Stdin(bytes[1..].to_vec()),
            ENV => {
                let mut name_length: [u8; 4] = [0, 0, 0, 0];
                name_length.copy_from_slice(&bytes[1..5]);
                let name_length = u32::from_be_bytes(name_length) as usize;
                let name_end = 5 + name_length;
                let name = Message::string_from_bytes(&bytes[5..name_end]);
                let value = Message::string_from_bytes(&bytes[name_end..]);
                Message::Env(name, value)
            }
            CURRENT_DIR => Message::CurrentDir(Message::string_from_bytes(&bytes[1..])),
            EOT => Message::Eot,
            _ => panic!("unexpected message {:?}", bytes),
        }
    }

    fn string_from_bytes(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn to_bytes(&self) -> Vec<u8> {
        match self {
            Message::Eot => vec![0, 0, 0, 1, EOT],
            Message::Error(message) => Message::to_vec(ERROR, message.as_bytes()),
            Message::Stdout(buffer) => Message::to_vec(STDOUT, buffer.as_slice()),
            Message::Stderr(buffer) => Message::to_vec(STDERR, buffer.as_slice()),
            Message::ExitStatus(code) => Message::to_vec(EXIT_STATUS, &code.to_be_bytes()),
            _ => panic!("{} cannot be encoded to bytes", self),
        }
    }

    fn to_vec(message_type: u8, bytes: &[u8]) -> Vec<u8> {
        let length = (1 + bytes.len()) as u32;
        let mut buffer: Vec<u8> = length.to_be_bytes().to_vec();
        buffer.push(message_type);
        buffer.extend(bytes);
        buffer
    }

    async fn read_from_erlang() -> io::Result<Message> {
        let mut stdin = tokio::io::stdin();
        let length = stdin.read_u32().await? as usize;

        let mut buffer: Vec<u8> = vec![0; length];
        stdin.read_exact(&mut buffer).await?;

        let message = Message::from_bytes(buffer);

        if unsafe { DEBUG } {
            eprint!("→ {:?}\r\n", message);
        }

        Ok(message)
    }

    async fn monitor_erlang() -> std::io::Error {
        loop {
            match Message::read_from_erlang().await {
                Err(error) if error.kind() == ErrorKind::UnexpectedEof => return error,
                _ => (),
            }
        }
    }

    async fn write_to_erlang(&self) {
        let mut stdout = tokio::io::stdout();
        stdout
            .write_all(&self.to_bytes())
            .await
            .expect("failed to write to erlang");
        stdout.flush().await.expect("failed to flush to erlang");

        if unsafe { DEBUG } {
            eprint!("← {:?}\r\n", self);
        }
    }

    async fn stream_to_child(mut stdin: ChildStdin, input: Option<Vec<u8>>) -> io::Result<()> {
        if let Some(input) = input {
            stdin.write_all(&input.as_slice()).await?;
            stdin.flush().await?;
        }
        Ok(())
    }

    async fn stream_to_erlang<S, F>(mut stream: S, create_message: F) -> io::Result<()>
    where
        S: AsyncRead,
        F: Fn(Vec<u8>) -> Message,
    {
        let mut buffer = vec![];
        while stream.read_buf(&mut buffer).await? > 0 {
            let message = create_message(buffer.clone());
            message.write_to_erlang().await;
            buffer.clear();
        }
        Ok(())
    }

    async fn send_error_to_erlang(error: std::io::Error) {
        if error.kind() != ErrorKind::UnexpectedEof {
            let message = format!("{}", error);
            let _ = Message::Error(message).write_to_erlang();
        }
    }
}

impl std::fmt::Display for Message {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        let name = match self {
            Message::Command(_) => "COMMAND",
            Message::Arg(_) => "ARG",
            Message::Stdin(_) => "STDIN",
            Message::Env(_, _) => "ENV",
            Message::CurrentDir(_) => "CURRENT_DIR",
            Message::Eot => "EOT",
            Message::Error(_) => "ERROR",
            Message::Stdout(_) => "STDOUT",
            Message::Stderr(_) => "STDERR",
            Message::ExitStatus(_) => "EXIT_STATUS",
        };
        write!(formatter, "{}", name)
    }
}

async fn receive_command() -> io::Result<(Command, Option<Vec<u8>>)> {
    let mut program: Option<String> = None;
    let mut args: Vec<String> = vec![];
    let mut stdin: Option<Vec<u8>> = None;
    let mut envs: HashMap<String, String> = HashMap::new();
    let mut current_dir: Option<String> = None;

    loop {
        match Message::read_from_erlang().await? {
            Message::Command(string) => program = Some(string),
            Message::Arg(string) => args.push(string),
            Message::Stdin(bytes) => stdin = Some(bytes),
            Message::Env(name, value) => {
                envs.insert(name, value);
            }
            Message::CurrentDir(string) => current_dir = Some(string),
            Message::Eot => break,
            message => panic!("unexpected message {}", message),
        }
    }

    let mut command = Command::new(program.expect("command required"));
    command
        .args(args)
        .envs(envs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }
    Ok((command, stdin))
}

async fn run_command(mut command: Command, input: Option<Vec<u8>>) -> io::Result<ExitStatus> {
    let monitor = Message::monitor_erlang().fuse();
    let mut monitor = Box::pin(monitor);

    let mut child = command.spawn()?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "failed to open child stdin"))?;
    let stdin = Message::stream_to_child(stdin, input).fuse();
    let mut stdin = Box::pin(stdin);

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "failed to open child stdout"))?;
    let stdout = Message::stream_to_erlang(stdout, Message::Stdout).fuse();
    let mut stdout = Box::pin(stdout);

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "failed to open child stderr"))?;
    let stderr = Message::stream_to_erlang(stderr, Message::Stderr).fuse();
    let mut stderr = Box::pin(stderr);

    let child = child.fuse();
    let mut child = Box::pin(child);

    let mut stdin_done = false;
    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut child_result: Option<io::Result<ExitStatus>> = None;

    while !stdin_done || !stdout_done || !stderr_done || child_result.is_none() {
        futures::select_biased! {
            error = monitor => return Err(error),
            result = stdin => stdin_done = true,
            result = stdout => stdout_done = true,
            result = stderr => stderr_done = true,
            result = child => child_result = Some(result),
        }
    }

    child_result.unwrap()
}

async fn run() -> io::Result<()> {
    let (command, input) = receive_command().await?;
    let status = run_command(command, input).await?;
    if let Some(code) = status.code() {
        Message::ExitStatus(code).write_to_erlang().await;
    }
    Ok(())
}

static mut DEBUG: bool = false;

#[tokio::main(basic_scheduler)]
async fn main() {
    unsafe {
        DEBUG = std::env::var_os("RAMBO_DEBUG").is_some();
    }

    match run().await {
        Ok(()) => Message::Eot.write_to_erlang().await,
        Err(error) => Message::send_error_to_erlang(error).await,
    };
}
